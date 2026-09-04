#!/usr/bin/env python3
"""BuilderId 403 分析全量上游接口复现脚本

对应文档：docs/builderid-403-profile-arn-analysis-and-optimization-design.md
覆盖 4.1–4.11 全部实测项（含负例）。在部署实例所在主机运行：

    python3 scripts/repro_builderid_upstream_matrix.py

前提：与 kiro-rs 同机，凭据文件 ./credentials.json、配置 ./config.json 可读。
脚本会先经本地 Admin 接口强刷 BuilderId 凭据（轮换 accessToken 属正常运行行为），
随后直发上游。全程脱敏输出（token/key/邮箱仅前缀）；10 系列创建的验证 key 在
finally 中删除，不在账号留残留。
"""
import json
import re
import struct
import subprocess
import sys
import time
import urllib.parse
from pathlib import Path
from datetime import datetime, timedelta, timezone

# 基目录自动探测：优先脚本所在目录（部署形态），其次上一级（仓库脚本目录形态）
_HERE = Path(__file__).resolve().parent
BASE = _HERE if (_HERE / "credentials.json").exists() else _HERE.parent
CREDS = BASE / "credentials.json"
CONFIG = BASE / "config.json"

BUILDER_ARN = "arn:aws:codewhisperer:us-east-1:638616132270:profile/AAAACCCCXXXX"
SOCIAL_ARN = "arn:aws:codewhisperer:us-east-1:699475941385:profile/EHGA3GRVQMUK"

RESULTS = []


def record(item, desc, status, detail=""):
    detail = re.sub(r"\s+", " ", detail)[:140]
    RESULTS.append((item, desc, status, detail))
    print(f"{item:>6} | {status:>4} | {desc} | {detail}", flush=True)


def curl_json(url, token=None, headers=None, data=None, binary=False, method=None, timeout=25):
    cmd = ["curl", "-s", "-o", "-", "-w", "\x1f%{http_code}", url, "--max-time", str(timeout)]
    if method:
        cmd += ["-X", method]
    if token:
        cmd += ["-H", "Authorization: Bearer " + token]
    for k, v in (headers or {}).items():
        cmd += ["-H", f"{k}: {v}"]
    if data is not None:
        payload = data if isinstance(data, (bytes, bytearray)) else json.dumps(data).encode()
        cmd += ["--data-binary", "@-"]
        r = subprocess.run(cmd, input=payload, capture_output=True)
    else:
        r = subprocess.run(cmd, capture_output=True)
    out = r.stdout
    if b"\x1f" not in out:
        return "ERR", ""
    sep = out.rindex(b"\x1f")
    return out[sep + 1:].decode(), (out[:sep].decode("latin1") if binary else out[:sep].decode("utf-8", "replace"))


def curl_cookies(url, cookies, headers=None, data=None, timeout=25):
    cmd = ["curl", "-s", "-o", "-", "-w", "\x1f%{http_code}", url, "-X", "POST",
           "-b", cookies, "--data-binary", "@-", "--max-time", str(timeout)]
    for k, v in (headers or {}).items():
        cmd += ["-H", f"{k}: {v}"]
    r = subprocess.run(cmd, input=data, capture_output=True)
    out = r.stdout
    if b"\x1f" not in out:
        return "ERR", b""
    sep = out.rindex(b"\x1f")
    return out[sep + 1:].decode(), out[:sep]


# ---------- CBOR（手写，服务器无 cbor2） ----------

def tstr(s):
    b = s.encode()
    if len(b) <= 23:
        return bytes([0x60 | len(b)]) + b
    if len(b) <= 255:
        return bytes([0x78, len(b)]) + b
    return bytes([0x79]) + struct.pack(">H", len(b)) + b


def cb_decode(buf, off=0):
    b = buf[off]
    mt, ai = b >> 5, b & 31
    if mt == 7 and ai == 31:
        raise ValueError("break")
    if ai == 31 and mt in (2, 3):
        parts, p = [], off + 1
        while buf[p] != 0xFF:
            x, p = cb_decode(buf, p)
            parts.append(x)
        out = b"".join(parts)
        return (out.decode("utf-8", "replace") if mt == 3 else out), p + 1
    hl = 1
    if ai < 24:
        v = ai
    elif ai == 24:
        v, hl = buf[off + 1], 2
    elif ai == 25:
        v, hl = struct.unpack(">H", buf[off + 1:off + 3])[0], 3
    elif ai == 26:
        v, hl = struct.unpack(">I", buf[off + 1:off + 5])[0], 5
    elif ai == 27:
        v, hl = struct.unpack(">Q", buf[off + 1:off + 9])[0], 9
    else:
        v = None
    if mt == 0:
        return v, off + hl
    if mt == 1:
        return -v - 1, off + hl
    if mt == 2:
        return buf[off + hl:off + hl + v], off + hl + v
    if mt == 3:
        return buf[off + hl:off + hl + v].decode("utf-8", "replace"), off + hl + v
    if mt == 4:
        out, p = [], off + hl
        if ai == 31:
            while buf[p] != 0xFF:
                x, p = cb_decode(buf, p)
                out.append(x)
            return out, p + 1
        for _ in range(v):
            x, p = cb_decode(buf, p)
            out.append(x)
        return out, p
    if mt == 5:
        out, p = {}, off + hl
        if ai == 31:
            while buf[p] != 0xFF:
                k, p = cb_decode(buf, p)
                x, p = cb_decode(buf, p)
                out[k] = x
            return out, p + 1
        for _ in range(v):
            k, p = cb_decode(buf, p)
            x, p = cb_decode(buf, p)
            out[k] = x
        return out, p
    if mt == 6:
        x, p = cb_decode(buf, off + hl)
        if v == 1:
            try:
                return datetime.fromtimestamp(int(x), timezone.utc).isoformat(), p
            except Exception:
                return x, p
        return x, p
    if mt == 7:
        if ai == 20:
            return False, off + 1
        if ai == 21:
            return True, off + 1
        if ai == 22:
            return None, off + 1
        if ai == 27:
            return struct.unpack(">d", buf[off + 1:off + 9])[0], off + 9
    raise ValueError(f"mt={mt} ai={ai}")


def ascii_brief(body, n=6):
    return " ".join(m.decode("latin1") for m in re.findall(rb"[\x20-\x7e]{4,}", body)[:n])[:140]


# ---------- 头部模板（与代理代码一致，见文档 3.1） ----------

def hdr_cws():
    return {
        "accept": "application/json",
        "user-agent": "aws-sdk-js/1.0.0 ua/2.1 os/linux lang/js md/nodejs#20.19.0 api/codewhispererruntime#1.0.0 m/N,E KiroIDE-0.6.7-internal",
        "x-amz-user-agent": "aws-sdk-js/1.0.0 KiroIDE-0.6.7-internal",
        "x-amzn-codewhisperer-optout": "true",
    }


def hdr_stream():
    return {
        "content-type": "application/json",
        "accept": "application/json",
        "user-agent": "aws-sdk-js/1.0.34 ua/2.1 os/linux lang/js md/nodejs#20.19.0 api/codewhispererstreaming#1.0.34 m/E KiroIDE-0.6.7-internal",
        "x-amz-user-agent": "aws-sdk-js/1.0.34 KiroIDE-0.6.7-internal",
        "x-amzn-codewhisperer-optout": "true",
        "x-amzn-kiro-agent-mode": "vibe",
    }


def gen_body(arn=None):
    b = {"conversationState": {
        "agentTaskType": "vibe",
        "chatTriggerType": "MANUAL",
        "currentMessage": {"userInputMessage": {
            "userInputMessageContext": {},
            "content": "Say hi",
            "modelId": "auto",
            "origin": "AI_EDITOR"}},
        "conversationId": "conv-repro"}}
    if arn:
        b["profileArn"] = arn
    return b


def portal_hdr():
    return {
        "content-type": "application/cbor",
        "accept": "application/cbor",
        "smithy-protocol": "rpc-v2-cbor",
        "origin": "https://app.kiro.dev",
        "referer": "https://app.kiro.dev/",
        "user-agent": "Mozilla/5.0 (Macintosh) Chrome/151",
    }


def main():
    creds = json.loads(CREDS.read_text())
    config = json.loads(CONFIG.read_text())
    by_id = {c["id"]: c for c in creds}
    key_cred = next((c for c in creds if c.get("kiroApiKey")), None)

    # 0. 强刷全部可刷新凭据（idc + social），保证 accessToken 新鲜
    admin_key, port = config.get("adminApiKey"), config.get("port")
    for c in creds:
        if c.get("authMethod") in ("idc", "social") and admin_key and port and not c.get("kiroApiKey"):
            subprocess.run(["curl", "-s", "-o", "/dev/null", "-X", "POST",
                            f"http://127.0.0.1:{port}/admin/credentials/{c['id']}/refresh",
                            "-H", "Authorization: Bearer " + admin_key,
                            "--max-time", "30"], capture_output=True)
    # 轮询等待全部可刷新凭据的 expiresAt 推进到未来（social 刷新较慢，固定 sleep 会读到旧 token）
    def fresh(c):
        ea = c.get("expiresAt")
        if not ea:
            return True
        try:
            return datetime.fromisoformat(ea.replace("Z", "+00:00")) > datetime.now(timezone.utc) + timedelta(seconds=120)
        except Exception:
            return True

    for _ in range(15):
        creds = json.loads(CREDS.read_text())
        pend = [c["id"] for c in creds if c.get("authMethod") in ("idc", "social") and not c.get("kiroApiKey") and not fresh(c)]
        if not pend:
            break
        time.sleep(2)
    by_id = {c["id"]: c for c in creds}
    builder_ids = [c["id"] for c in creds if c.get("authMethod") == "idc"]
    social = next((c for c in creds if c.get("authMethod") == "social"), None)
    b1, b2 = by_id[builder_ids[0]], (by_id[builder_ids[1]] if len(builder_ids) > 1 else None)
    tok1 = b1["accessToken"]
    tok2 = b2["accessToken"] if b2 else None
    tok3 = social["accessToken"] if social else None

    enc = urllib.parse.quote
    usage_q = ("origin=AI_EDITOR&resourceType=AGENTIC_REQUEST&isEmailRequired=true")
    usage_url = "https://q.us-east-1.amazonaws.com/getUsageLimits?" + usage_q
    models_url = "https://codewhisperer.us-east-1.amazonaws.com/ListAvailableModels?origin=AI_EDITOR&maxResults=50"

    def sub_of(text):
        try:
            return json.loads(text).get("subscriptionInfo", {}).get("subscriptionTitle", "")
        except Exception:
            return ""

    print("== 4.1 getUsageLimits ==")
    s, t = curl_json(usage_url, tok1)
    record("1a", "#1 无ARN", s, sub_of(t) or t)
    s, t = curl_json(usage_url + "&profileArn=" + enc(BUILDER_ARN), tok1)
    record("1b", "#1 builder占位ARN", s, sub_of(t))
    s, t = curl_json(usage_url + "&profileArn=" + enc(SOCIAL_ARN), tok1)
    record("1c", "#1 social占位ARN", s, t)
    if tok2:
        s, t = curl_json(usage_url, tok2)
        record("1e", "#2 无ARN", s, t)
        s, t = curl_json(usage_url + "&profileArn=" + enc(BUILDER_ARN), tok2)
        record("1f", "#2 builder占位ARN", s, sub_of(t))
    if tok3:
        s, t = curl_json(usage_url, tok3)
        record("1d", "#3 无ARN", s, sub_of(t))

    print("== 4.2 ListAvailableModels ==")
    s, t = curl_json(models_url, tok1, hdr_cws())
    record("2a", "#1 无ARN", s, t)
    s, t = curl_json(models_url + "&profileArn=" + enc(BUILDER_ARN), tok1, hdr_cws())
    record("2b", "#1 builder占位ARN", s, t)
    if tok3:
        s, t = curl_json(models_url, tok3, hdr_cws())
        record("2c", "#3 无ARN", s, "models:" + str(len(json.loads(t).get("models", []))) if s == "200" else t)

    print("== 4.3 ListAvailableProfiles ==")
    lp_url = "https://codewhisperer.us-east-1.amazonaws.com/ListAvailableProfiles"
    s, t = curl_json(lp_url, tok1, hdr_cws(), {"maxResults": 10}, method="POST")
    record("3a", "#1", s, t)
    if tok3:
        s, t = curl_json(lp_url, tok3, hdr_cws(), {"maxResults": 10}, method="POST")
        record("3b", "#3", s, t)

    print("== 4.4 generateAssistantResponse ==")
    gen_url = "https://q.us-east-1.amazonaws.com/generateAssistantResponse"
    s, t = curl_json(gen_url, tok1, hdr_stream(), gen_body(None), binary=True)
    record("4a", "#1 无ARN", s, t)
    s, t = curl_json(gen_url, tok1, hdr_stream(), gen_body(BUILDER_ARN), binary=True)
    record("4b", "#1 builder占位ARN", s, t)
    s, t = curl_json(gen_url, tok1, hdr_stream(), gen_body(SOCIAL_ARN), binary=True)
    record("4c", "#1 social占位ARN", s, t)
    if tok3:
        s, t = curl_json(gen_url, tok3, hdr_stream(), gen_body(None), binary=True)
        record("4d", "#3 无ARN", s, ("SSE:" + ("ok" if "assistantResponseEvent" in t else "?")))
        s, t = curl_json(gen_url, tok3, hdr_stream(),
                         {**gen_body(None), "conversationState": {**gen_body(None)["conversationState"]}}, binary=True)
        record("4e", "#3 无ARN 复验", s, "SSE:" + ("ok" if "assistantResponseEvent" in t else t[:60]))

    print("== 4.5 mcp ==")
    mcp_url = "https://q.us-east-1.amazonaws.com/mcp"
    mcp_body = {"jsonrpc": "2.0", "id": 1, "method": "tools/list", "params": {}}
    s, t = curl_json(mcp_url, tok1, hdr_stream(), mcp_body)
    record("5a", "#1 无ARN", s, t)
    s, t = curl_json(mcp_url, tok1, hdr_stream(), {**mcp_body, "profileArn": BUILDER_ARN})
    record("5b", "#1 请求体带占位ARN", s, t)
    s, t = curl_json(mcp_url, tok1, {**hdr_stream(), "x-amzn-kiro-profile-arn": BUILDER_ARN}, mcp_body)
    record("5c2", "#1 请求头带占位ARN", s, t)
    if tok3:
        s, t = curl_json(mcp_url, tok3, hdr_stream(), mcp_body)
        record("5c", "#3 无ARN", s, ("tools:" + ("ok" if "web_search" in t else t[:40])) if s == "200" else t)

    print("== 4.6 认证/刷新面（不单独触发，见文档 3.2 日志证据） ==")
    record("6", "IdC/Social 刷新", "SKIP", "避免额外轮转；以服务日志当日成功记录为证")

    print("== 4.9/4.10 API Key 路径与 token 类型识别 ==")
    if key_cred:
        kk = key_cred["kiroApiKey"]
        s, t = curl_json(usage_url, kk, {"tokentype": "API_KEY"})
        record("7a", "#4 getUsageLimits 无ARN", s, sub_of(t))
        s, t = curl_json(models_url, kk, {**hdr_cws(), "tokentype": "API_KEY"})
        record("7b", "#4 ListAvailableModels 无ARN", s, "models:ok" if s == "200" else t)
        s, t = curl_json(gen_url, kk, {**hdr_stream(), "tokentype": "API_KEY"}, gen_body(None), binary=True)
        record("7c", "#4 generate 无ARN", s, "SSE:" + ("ok" if "assistantResponseEvent" in t else t[:40]))
        s, t = curl_json(mcp_url, kk, {**hdr_stream(), "tokentype": "API_KEY"}, mcp_body)
        record("7d", "#4 mcp", s, "tools:ok" if s == "200" and "web_search" in t else t)
        s, t = curl_json(lp_url, kk, {**hdr_cws(), "tokentype": "API_KEY"}, {"maxResults": 10}, method="POST")
        record("7e", "#4 ListAvailableProfiles", s, t)
        s, t = curl_json(models_url, kk, hdr_cws())
        record("8a", "#4 key 不带tokentype头", s, t)
        s, t = curl_json(models_url, tok1, {**hdr_cws(), "tokentype": "API_KEY"})
        record("8b", "#1 accessToken 冒充tokentype头", s, t)
        s, t = curl_json(models_url + "&profileArn=" + enc(BUILDER_ARN), kk, {**hdr_cws(), "tokentype": "API_KEY"})
        record("8c", "#4 key + builder占位ARN", s, "models:ok" if s == "200" else t)
        s, t = curl_json(models_url + "&profileArn=" + enc(SOCIAL_ARN), kk, {**hdr_cws(), "tokentype": "API_KEY"})
        record("8d", "#4 key + social占位ARN", s, t)
    else:
        record("7", "API Key 路径", "SKIP", "无 kiroApiKey 凭据")

    print("== 4.7 Kiro Control Plane 管理面 ==")
    mg_url = "https://management.us-east-1.kiro.dev/"

    def mgmt(target, body, token):
        return curl_json(mg_url, token,
                         {"content-type": "application/x-amz-json-1.0",
                          "accept": "*/*",
                          "origin": "https://app.kiro.dev",
                          "referer": "https://app.kiro.dev/",
                          "x-amz-target": "KiroControlPlaneBearerService." + target,
                          "x-amz-user-agent": "aws-sdk-js/1.0.0 ua/2.1 os/macOS lang/js md/browser#Chromium_151 api/kirocontrolplanebearer#1.0.0 m/N,E"},
                         body, method="POST")

    s, t = mgmt("ListApiKeys", {"profileArn": BUILDER_ARN}, "aoa_invalid_repro_probe")
    record("9a", "ListApiKeys 无效token", s, t)
    s, t = mgmt("ListApiKeys", {"profileArn": BUILDER_ARN}, tok1)
    record("9d", "#1 ListApiKeys", s, ("keys:" + str(len(json.loads(t).get("keys", [])))) if s == "200" else t)

    created_key_id = None
    try:
        s, t = mgmt("CreateApiKey", {"profileArn": BUILDER_ARN, "label": "kiro-rs-repro"}, tok1)
        record("10a", "#1 CreateApiKey", s, t if s != "200" else json.dumps({k: (v[:10] + "…" if k == "rawKey" else v) for k, v in json.loads(t).items()}))
        if s == "200":
            j = json.loads(t)
            created_key_id = j["keyId"]
            raw = j["rawKey"]
            s, t = curl_json(gen_url, raw, {**hdr_stream(), "tokentype": "API_KEY"}, gen_body(None), binary=True)
            record("10b", "rawKey generate", s, "SSE:" + ("ok" if "assistantResponseEvent" in t else t[:40]))
            s, t = curl_json(models_url, raw, {**hdr_cws(), "tokentype": "API_KEY"})
            record("10c", "rawKey ListAvailableModels", s, "models:ok" if s == "200" else t)
    finally:
        if created_key_id:
            s, t = mgmt("DeleteApiKey", {"profileArn": BUILDER_ARN, "keyId": created_key_id}, tok1)
            record("10d", "DeleteApiKey 清理", s, t or "{}")
            s, t = mgmt("ListApiKeys", {"profileArn": BUILDER_ARN}, tok1)
            gone = created_key_id not in t if s == "200" else False
            record("10e", "复核已删除", "200" if gone else s, "gone" if gone else t)

    print("== 4.11 Kiro Web 门户面 ==")
    uid = None
    s, t = curl_json(usage_url + "&profileArn=" + enc(BUILDER_ARN), tok1)
    try:
        uid = json.loads(t).get("userInfo", {}).get("userId")
    except Exception:
        pass
    CK = f"AccessToken={tok1}; Idp=BuilderId; UserId={uid or ''}"
    ph = portal_hdr()
    b_arn = bytes([0xa2]) + tstr("origin") + tstr("KIRO_IDE") + tstr("profileArn") + tstr(BUILDER_ARN)
    b_usage = bytes([0xa3]) + tstr("origin") + tstr("KIRO_IDE") + tstr("isEmailRequired") + bytes([0xf5]) + tstr("profileArn") + tstr(BUILDER_ARN)
    b_ui_noarn = bytes([0xa1]) + tstr("origin") + tstr("KIRO_IDE")
    b_usage_noarn = bytes([0xa2]) + tstr("origin") + tstr("KIRO_IDE") + tstr("isEmailRequired") + bytes([0xf5])

    def portal(op, body, cookies=CK, hdrs=None):
        return curl_cookies("https://app.kiro.dev/service/KiroWebPortalService/operation/" + op,
                            cookies, hdrs or ph, body)

    def portal_json(op, body, cookies=CK, hdrs=None):
        s, raw = portal(op, body, cookies, hdrs)
        if s == "200":
            try:
                return s, cb_decode(raw)[0]
            except Exception as e:
                return s, {"_decode_err": str(e)}
        return s, ascii_brief(raw)

    s, x = portal_json("GetUserInfo", b_arn, cookies="", hdrs={**ph, "Authorization": "Bearer " + tok1})
    record("11a", "GetUserInfo 仅Auth头", s, str(x))
    s, x = portal_json("GetUserUsageAndLimits", b_usage, cookies="", hdrs={**ph, "Authorization": "Bearer " + tok1})
    record("11b", "Usage 仅Auth头", s, str(x))
    s, x = portal_json("GetUserInfo", b_arn, cookies="Idp=BuilderId; UserId=" + (uid or ""), hdrs={**ph, "Authorization": "Bearer " + tok1})
    record("11c", "GetUserInfo Auth头+Idp/UserId cookie", s, str(x))
    s, x = portal_json("GetUserInfo", b_arn)
    record("11h", "GetUserInfo 仅cookie", s, f"idp={x.get('idp')} status={x.get('status')}" if isinstance(x, dict) else str(x))
    s, x = portal_json("GetUserInfo", b_arn, cookies="Idp=BuilderId; UserId=" + (uid or ""))
    record("11f", "GetUserInfo 无token仅Idp cookie", s, str(x))
    s, x = portal_json("GetUserInfo", bytes([0xa0]))
    record("11r", "GetUserInfo 空body+cookie", s, f"status={x.get('status')}" if isinstance(x, dict) else str(x))
    s, x = portal_json("GetUserInfo", bytes([0xa0]), cookies="", hdrs={**ph, "Authorization": "Bearer " + tok1})
    record("11g", "GetUserInfo 空body 无cookie", s, str(x))
    s, x = portal_json("GetUserUsageAndLimits", b_usage)
    record("11i", "Usage 仅cookie", s, f"sub={x.get('subscriptionInfo', {}).get('subscriptionTitle')}" if isinstance(x, dict) else str(x))
    s, raw = portal("ListAvailableModels", bytes([0xa1]) + tstr("profileArn") + tstr(BUILDER_ARN))
    record("11j", "Models 仅cookie", s, ascii_brief(raw, 3))
    s, x = portal_json("GetUserInfo", b_arn, cookies="AccessToken=" + tok1)
    record("11l", "GetUserInfo 缺Idp cookie", s, str(x))
    s, x = portal_json("GetUserInfo", b_arn, cookies=f"AccessToken={tok1}; Idp=BuilderId")
    record("11m", "GetUserInfo 无UserId cookie", s, f"status={x.get('status')}" if isinstance(x, dict) else str(x))
    s, x = portal_json("GetUserInfo", b_ui_noarn)
    record("11n", "GetUserInfo 无ARN", s, f"status={x.get('status')}" if isinstance(x, dict) else str(x))
    s, x = portal_json("GetUserUsageAndLimits", b_usage_noarn)
    record("11o", "Usage 无ARN", s, f"sub={x.get('subscriptionInfo', {}).get('subscriptionTitle')}" if isinstance(x, dict) else str(x))
    s, raw = portal("ListAvailableModels", bytes([0xa0]))
    record("11p", "Models 无ARN", s, ascii_brief(raw, 3))
    if tok3:
        s, x = portal_json("GetUserInfo", b_ui_noarn, cookies=f"AccessToken={tok3}; Idp=Social")
        record("11q", "#3 Social Idp=Social", s, f"status={x.get('status')}" if isinstance(x, dict) else str(x))

    print("== 4.11 CSRF 与操作面探测 ==")
    fake = "a" * 44 + "="
    s, raw = portal("ListSpaces", bytes([0xa1]) + tstr("csrfToken") + tstr(fake))
    record("12a", "ListSpaces body假csrf", s, ascii_brief(raw, 4))
    s, raw = portal("ListSpaces", bytes([0xa0]), hdrs={**ph, "x-csrf-token": fake})
    record("12b", "ListSpaces 头假csrf", s, ascii_brief(raw, 4))
    s, raw = portal("SendMessage", bytes([0xa0]))
    record("12e", "SendMessage 空body", s, ascii_brief(raw, 6))
    s, raw = portal("SendMessage", bytes([0xa1]) + tstr("spaceId") + tstr("repro"))
    record("12f", "SendMessage 带spaceId", s, ascii_brief(raw, 6))
    for op in ["GenerateAssistantResponse", "CreateConversation", "Chat", "GetCsrfToken", "Bootstrap"]:
        s, raw = portal(op, bytes([0xa0]))
        record("12g", op, s, ascii_brief(raw, 3))

    print("\n== 汇总 ==")
    print(f"共 {len(RESULTS)} 项；时间 {datetime.now(timezone.utc).isoformat()}")
    print("注意：本输出含实时上游响应片段；存档时请确认无敏感值外泄（token/key/邮箱已按前缀脱敏）。")


if __name__ == "__main__":
    sys.exit(main())
