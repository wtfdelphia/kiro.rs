//! ServerControl 状态机测试（任务 2.4：真实 tokio）

use futures::StreamExt;
use tokio::sync::oneshot;

use super::server::{ServerControl, ServerStatus};
use crate::bridge::{CoreEvent, event_channel};

/// 装配一个最小可启动的控制器：路由只挂一个固定返回的端点
fn control_with_router() -> (ServerControl, futures::channel::mpsc::UnboundedReceiver<CoreEvent>) {
    let (tx, rx) = event_channel();
    let ctl = ServerControl::new(tokio::runtime::Handle::current(), tx);
    let app = axum::Router::new().route(
        "/ping",
        axum::routing::get(|| async { "pong" }),
    );
    let (ws, _) = tokio::sync::broadcast::channel::<()>(1);
    ctl.attach(app, ws);
    (ctl, rx)
}

/// 从事件流里收集到目标状态为止（带超时，防挂死）
async fn wait_status(
    rx: &mut futures::channel::mpsc::UnboundedReceiver<CoreEvent>,
    want: &ServerStatus,
) -> ServerStatus {
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let left = deadline - tokio::time::Instant::now();
        let evt = tokio::time::timeout(left, rx.next())
            .await
            .expect("等待状态超时")
            .expect("事件通道关闭");
        if let CoreEvent::ServerStatus(s) = evt {
            if std::mem::discriminant(&s) == std::mem::discriminant(want) {
                return s;
            }
        }
    }
}

#[tokio::test]
async fn start_bind_and_serve() {
    let (ctl, mut rx) = control_with_router();
    assert!(ctl.start("127.0.0.1:0"), "Stopped 态应可启动");

    let status = wait_status(&mut rx, &ServerStatus::Running(String::new())).await;
    let ServerStatus::Running(addr) = status else { panic!("应为 Running") };
    // 端口 0 时记录的是实际分配端口
    assert!(addr.starts_with("127.0.0.1:"), "地址应带回真实端口: {}", addr);
    assert!(!addr.ends_with(":0"), "不应残留占位端口");

    // 真实命中内嵌服务器
    let url = format!("http://{}/ping", addr);
    let body = reqwest::get(&url).await.unwrap().text().await.unwrap();
    assert_eq!(body, "pong");

    // 运行中重复 start 无效
    assert!(!ctl.start("127.0.0.1:0"), "Running 态不应再启动");
    ctl.request_stop().await.unwrap();
    assert_eq!(ctl.status(), ServerStatus::Stopped);
}

#[tokio::test]
async fn port_conflict_goes_failed_and_retries() {
    // 占住一个端口制造冲突
    let occupied = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let taken = occupied.local_addr().unwrap();

    let (ctl, mut rx) = control_with_router();
    assert!(ctl.start(&taken.to_string()));
    let status = wait_status(&mut rx, &ServerStatus::Failed(String::new())).await;
    let ServerStatus::Failed(msg) = status else { panic!("应为 Failed") };
    assert!(msg.contains(&taken.to_string()), "失败信息应含地址: {}", msg);

    // Failed 事件发出后循环还在收尾（`loop_active` 复位在同一任务内稍后
    // 发生）；用停止回执做同步点，回执就绪即循环已退出、可再启动
    ctl.request_stop().await.unwrap();

    // Failed 态可重试：释放占位端口后重启成功
    drop(occupied);
    assert!(ctl.start(&taken.to_string()), "Failed 态应可重试");
    let status = wait_status(&mut rx, &ServerStatus::Running(String::new())).await;
    assert!(matches!(status, ServerStatus::Running(_)));
    ctl.request_stop().await.unwrap();
}

#[tokio::test]
async fn stop_receipt_and_drained_event() {
    let (ctl, mut rx) = control_with_router();
    ctl.start("127.0.0.1:0");
    wait_status(&mut rx, &ServerStatus::Running(String::new())).await;

    // 回执在收敛完成后可用
    ctl.request_stop().await.unwrap();
    assert_eq!(ctl.status(), ServerStatus::Stopped);

    // 未运行时停止：回执立即就绪
    let (tx, _) = oneshot::channel::<()>();
    drop(tx);
    let rx2 = ctl.request_stop();
    tokio::time::timeout(std::time::Duration::from_secs(2), rx2)
        .await
        .unwrap()
        .unwrap();

    // 事件流里能看到 ServerDrained
    let mut saw_drained = false;
    while let Ok(Some(evt)) =
        tokio::time::timeout(std::time::Duration::from_secs(2), rx.next()).await
    {
        if matches!(evt, CoreEvent::ServerDrained) {
            saw_drained = true;
            break;
        }
    }
    assert!(saw_drained, "停止后应发出 ServerDrained");
}

#[tokio::test]
async fn restart_rebinds_new_address() {
    let (ctl, mut rx) = control_with_router();
    ctl.start("127.0.0.1:0");
    let first = wait_status(&mut rx, &ServerStatus::Running(String::new())).await;
    let ServerStatus::Running(addr1) = first else { panic!() };

    // 重启到新的随机端口
    assert!(ctl.restart(Some("127.0.0.1:0")));
    // 重启路径：Stopping → Starting → Running（不再经过 Stopped 中间态）
    let second = wait_status(&mut rx, &ServerStatus::Running(String::new())).await;
    let ServerStatus::Running(addr2) = second else { panic!() };
    assert_ne!(addr1, addr2, "重启后应绑定新地址");

    // 新地址可命中
    let url = format!("http://{}/ping", addr2);
    let body = reqwest::get(&url).await.unwrap().text().await.unwrap();
    assert_eq!(body, "pong");
    ctl.request_stop().await.unwrap();
}

/// 重启窗口竞态回归：「应用并重启」后紧接退出，回执必须在服务循环真正
/// 退出后才兑现，收敛完成后状态是 `Stopped` 而不是重启中的新实例
///（修复前：重启间隙的 `Stopped` 中间态会让回执立即兑现，退出协调
/// 在服务器正按新地址重启时放行 `cx.quit()`）。
#[tokio::test]
async fn stop_after_restart_converges_to_stopped() {
    let (ctl, mut rx) = control_with_router();
    ctl.start("127.0.0.1:0");
    let first = wait_status(&mut rx, &ServerStatus::Running(String::new())).await;
    let ServerStatus::Running(addr1) = first else { panic!("应为 Running") };

    // 重启到新的随机端口，然后立即停止：停止请求可能落在重启间隙的
    // 任何窗口（Stopping / Starting）
    assert!(ctl.restart(Some("127.0.0.1:0")));
    let ack = ctl.request_stop();

    // 回执兑现后循环必须已退出：状态收敛为 Stopped，而非仍在重启
    tokio::time::timeout(std::time::Duration::from_secs(5), ack)
        .await
        .expect("停止回执超时")
        .unwrap();
    assert_eq!(
        ctl.status(),
        ServerStatus::Stopped,
        "回执兑现后不应残留重启中的状态（第一地址 {}）",
        addr1
    );

    // 循环已退出：可再次启动
    assert!(ctl.start("127.0.0.1:0"), "收敛后应可再启动");
    wait_status(&mut rx, &ServerStatus::Running(String::new())).await;
    ctl.request_stop().await.unwrap();
}

/// 停止优先于重启：收敛窗口内先请求停止、后到重启请求时按停止收敛
#[tokio::test]
async fn stop_beats_restart_in_drain_window() {
    let (ctl, mut rx) = control_with_router();
    ctl.start("127.0.0.1:0");
    wait_status(&mut rx, &ServerStatus::Running(String::new())).await;

    // 先停（置位停止标志），再重启（置位重启标志）
    let ack = ctl.request_stop();
    assert!(ctl.restart(Some("127.0.0.1:0")));

    tokio::time::timeout(std::time::Duration::from_secs(5), ack)
        .await
        .expect("停止回执超时")
        .unwrap();
    assert_eq!(ctl.status(), ServerStatus::Stopped, "停止应优先于重启");
}

#[test]
fn status_labels_are_stable() {
    assert_eq!(ServerStatus::Stopped.label(), "未启动");
    assert_eq!(ServerStatus::Starting.label(), "启动中");
    assert_eq!(ServerStatus::Running("x".into()).label(), "运行中");
    assert_eq!(ServerStatus::Stopping.label(), "停止中");
    assert_eq!(ServerStatus::Failed("e".into()).label(), "启动失败");
}
