//! 系统托盘（change 6，设计文档 §6 / design D1–D3、D6）
//!
//! 后端 `tray-icon 0.25 + ksni`：纯 D-Bus StatusNotifierItem，自管工作
//! 线程，无 GTK 依赖（证据 `evidence/tray-icon-0.25-ksni.md`）。事件经
//! 全局 `crossbeam_channel` 回传，GPUI 侧 200ms 轮询 `try_recv`。
//!
//! 常驻模型（design D2）：关窗拦截时最小化，托盘单击 / 「显示主窗口」
//! 经 `activate_window()` 唤回。退出入口（托盘「退出」）与其它三入口
//! 统一收敛 `quit::begin_phase1`（design D6）。
//!
//! 无 SNI 宿主（如 Xvfb）时 `TrayIcon` 创建失败，[`Tray::init`] 降级返回
//! 图标缺席的实例，其余路径（常驻、退出、服务器）不受影响。
//!
//! 注意：`tray_icon::TrayIcon` 内部是 `Rc<RefCell<…>>`，`!Send`，只能
//! 驻留 GPUI 主线程（事件循环闭包内），不挂需跨线程的 `CoreHandle`。

use futures::channel::mpsc;
use gpui_kit::gpui;
use tray_icon::menu::{
    CheckMenuItem, Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem,
};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

use crate::bridge::CoreEvent;
use crate::core::CoreHandle;
use crate::core::server::ServerStatus;
use crate::store::prefs;
use crate::{autostart, quit};

/// 图标边长（像素，RGBA）
pub const ICON_SIZE: u32 = 22;

/// 事件轮询间隔（主事件循环 `select!` 交织，见 `main`）
pub const POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(200);

const MENU_SHOW: &str = "tray.show";
const MENU_START: &str = "tray.start";
const MENU_STOP: &str = "tray.stop";
const MENU_AUTOSTART: &str = "tray.autostart";
const MENU_QUIT: &str = "tray.quit";

/// 托盘图标四态（design D3）
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum IconState {
    /// 服务器运行中（绿）
    Running,
    /// 已停止 / 启停过渡（灰）
    Stopped,
    /// 启动失败（红）
    Failed,
    /// 退出中（暗灰）
    Quitting,
}

impl IconState {
    fn from_status(status: &ServerStatus) -> Self {
        match status {
            ServerStatus::Running(_) => IconState::Running,
            ServerStatus::Failed(_) => IconState::Failed,
            _ => IconState::Stopped,
        }
    }
}

/// 运行时生成 22x22 RGBA：实心圆点 + 透明底（design D3，不引图片资源）
pub fn icon_rgba(state: IconState) -> Vec<u8> {
    let (r, g, b) = match state {
        IconState::Running => (76, 175, 80),
        IconState::Stopped => (158, 158, 158),
        IconState::Failed => (244, 67, 54),
        IconState::Quitting => (110, 110, 110),
    };
    let size = ICON_SIZE as f32;
    let center = size / 2.0;
    let radius = center - 2.0;
    let mut rgba = Vec::with_capacity((ICON_SIZE * ICON_SIZE * 4) as usize);
    for y in 0..ICON_SIZE {
        for x in 0..ICON_SIZE {
            let dx = x as f32 + 0.5 - center;
            let dy = y as f32 + 0.5 - center;
            if (dx * dx + dy * dy).sqrt() <= radius {
                rgba.extend_from_slice(&[r, g, b, 255]);
            } else {
                rgba.extend_from_slice(&[0, 0, 0, 0]);
            }
        }
    }
    rgba
}

/// 标题行文案（禁用态菜单项）
fn title_text(status: &ServerStatus) -> String {
    match status {
        ServerStatus::Running(addr) => format!("kiro-rs · 服务器: 运行中 ({})", addr),
        _ => format!("kiro-rs · 服务器: {}", status.label()),
    }
}

/// 构建托盘菜单（design D3：整体重建，不逐项更新）
///
/// 启动/停止按状态二选一；`Starting` / `Stopping` 过渡态禁用对应项。
pub fn build_menu(status: &ServerStatus, launch_at_login: bool) -> Menu {
    let title = MenuItem::with_id(MenuId::new("tray.title"), title_text(status), false, None);
    let show = MenuItem::with_id(MenuId::new(MENU_SHOW), "显示主窗口", true, None);

    let server_item: Box<dyn tray_icon::menu::IsMenuItem> = match status {
        ServerStatus::Running(_) => Box::new(MenuItem::with_id(
            MenuId::new(MENU_STOP),
            "停止服务器",
            true,
            None,
        )),
        ServerStatus::Stopped | ServerStatus::Failed(_) => Box::new(MenuItem::with_id(
            MenuId::new(MENU_START),
            "启动服务器",
            true,
            None,
        )),
        ServerStatus::Starting => Box::new(MenuItem::with_id(
            MenuId::new(MENU_START),
            "启动服务器",
            false,
            None,
        )),
        ServerStatus::Stopping => Box::new(MenuItem::with_id(
            MenuId::new(MENU_STOP),
            "停止服务器",
            false,
            None,
        )),
    };

    let autostart = CheckMenuItem::with_id(
        MenuId::new(MENU_AUTOSTART),
        "开机自启",
        true,
        launch_at_login,
        None,
    );
    let sep = PredefinedMenuItem::separator();
    let quit_item = MenuItem::with_id(MenuId::new(MENU_QUIT), "退出", true, None);

    let menu = Menu::new();
    let _ = menu.append_items(&[
        &title,
        &show,
        server_item.as_ref(),
        &autostart,
        &sep,
        &quit_item,
    ]);
    menu
}

/// 托盘控制器（驻留主线程）
///
/// `icon` 为 `None` 表示无 SNI 宿主 / 创建失败的降级态；菜单与图标更新
/// 在 `icon` 缺席时静默跳过。动作分发需要 `CoreHandle` / 事件通道 /
/// 窗口句柄，装配完成（`Bootstrapped`）后经 [`attach`] 注入。
pub struct Tray {
    icon: Option<TrayIcon>,
    status: ServerStatus,
    /// 动作上下文（装配完成前为 `None`，菜单事件先忽略）
    action: Option<ActionCtx>,
}

#[derive(Clone)]
struct ActionCtx {
    handle: CoreHandle,
    events: mpsc::UnboundedSender<CoreEvent>,
    window: gpui::AnyWindowHandle,
}

/// 动作上下文在装配后一次性构建，菜单重建与事件分发共用。
impl Tray {
    /// 创建托盘。失败（无 SNI 宿主等）降级为图标缺席，不阻断启动
    pub fn init() -> Self {
        let status = ServerStatus::Stopped;
        let menu = build_menu(&status, false);
        let icon = match TrayIconBuilder::new()
            .with_icon(icon_from_state(&status))
            .with_tooltip("kiro-rs")
            .with_menu(Box::new(menu))
            .build()
        {
            Ok(icon) => Some(icon),
            Err(e) => {
                tracing::warn!("托盘创建失败，降级为无托盘（常驻与退出仍可用）: {}", e);
                None
            }
        };
        Self {
            icon,
            status,
            action: None,
        }
    }

    /// 注入动作上下文（`Bootstrapped` 后调用），并按真实偏好重建菜单
    pub fn attach(&mut self, handle: CoreHandle, events: mpsc::UnboundedSender<CoreEvent>, window: gpui::AnyWindowHandle) {
        let launch = read_launch_at_login(&handle);
        self.action = Some(ActionCtx { handle, events, window });
        self.refresh(launch);
    }

    /// 服务器状态变化：切换图标 + 重建菜单（design D3）
    pub fn on_server_status(&mut self, status: ServerStatus) {
        self.status = status.clone();
        let launch = self
            .action
            .as_ref()
            .map(|a| read_launch_at_login(&a.handle))
            .unwrap_or(false);
        self.refresh(launch);
    }

    fn refresh(&mut self, launch_at_login: bool) {
        let Some(icon) = &self.icon else {
            return;
        };
        if let Err(e) = icon.set_icon(Some(icon_from_state(&self.status))) {
            tracing::warn!("托盘图标更新失败: {}", e);
        }
        icon.set_menu(Some(Box::new(build_menu(&self.status, launch_at_login))));
    }

    /// 是否已降级（无图标）
    #[cfg(test)]
    pub fn degraded(&self) -> bool {
        self.icon.is_none()
    }

    /// 轮询托盘 / 菜单事件并分发（200ms 调一次，见 `main`）
    pub fn poll(&mut self, cx: &mut gpui::AsyncApp) {
        let Some(icon) = &self.icon else {
            return;
        };
        let icon_id = icon.id().clone();

        // 托盘图标单击 → 唤回主窗口
        while let Ok(ev) = TrayIconEvent::receiver().try_recv() {
            if let TrayIconEvent::Click {
                id,
                button,
                button_state,
                ..
            } = ev
            {
                if id == icon_id && button == MouseButton::Left && button_state == MouseButtonState::Up
                {
                    self.activate_window(cx);
                }
            }
        }

        // 菜单项激活
        while let Ok(ev) = MenuEvent::receiver().try_recv() {
            self.on_menu_event(ev.id().clone(), cx);
        }
    }

    fn on_menu_event(&mut self, id: MenuId, cx: &mut gpui::AsyncApp) {
        match id.0.as_str() {
            MENU_SHOW => self.activate_window(cx),
            MENU_START => self.start_server(),
            MENU_STOP => self.stop_server(),
            MENU_AUTOSTART => self.toggle_autostart(),
            MENU_QUIT => self.begin_quit(cx),
            _ => {}
        }
    }

    fn activate_window(&self, cx: &mut gpui::AsyncApp) {
        let Some(action) = &self.action else {
            return;
        };
        let _ = action.window.update(cx, |_, window, _| {
            window.activate_window();
        });
    }

    fn start_server(&self) {
        let Some(action) = &self.action else {
            return;
        };
        let settings = action.handle.service.get_server_settings();
        let addr = format!("{}:{}", settings.host, settings.port);
        if let Some(server) = action.handle.server() {
            server.start(&addr);
        }
    }

    fn stop_server(&self) {
        let Some(action) = &self.action else {
            return;
        };
        if let Some(server) = action.handle.server() {
            server.request_stop();
        }
    }

    fn toggle_autostart(&mut self) {
        let Some(action) = self.action.clone() else {
            return;
        };
        let current = read_launch_at_login(&action.handle);
        let next = !current;
        // 先写平台入口，成功才落库（design D4）
        match autostart::set_enabled(next) {
            Ok(()) => {
                if let Some(store) = action.handle.store() {
                    if let Err(e) = store.set_preference(prefs::LAUNCH_AT_LOGIN, next) {
                        tracing::warn!("写入 launch_at_login 失败: {}", e);
                    }
                }
                self.refresh(next);
            }
            Err(e) => {
                tracing::warn!("设置开机自启失败（入口写入失败，偏好不变）: {}", e);
                // 宿主可能已翻转勾选态：按落库值重建菜单，消除不一致；
                // 托盘无通知通道，日志是这条失败路径的唯一用户可见线索
                self.refresh(current);
            }
        }
    }

    fn begin_quit(&mut self, cx: &mut gpui::AsyncApp) {
        let Some(action) = self.action.clone() else {
            return;
        };
        if quit::is_quitting() {
            return;
        }
        let began = cx.update(|cx| {
            quit::begin_phase1(action.handle.clone(), action.events.clone(), cx)
        });
        if began {
            // 退出中图标切暗（后续 ServerStatus 事件也会持续刷新）
            let launch = read_launch_at_login(&action.handle);
            self.refresh(launch);
        }
    }
}

fn icon_from_state(status: &ServerStatus) -> Icon {
    let state = if quit::is_quitting() {
        IconState::Quitting
    } else {
        IconState::from_status(status)
    };
    // 尺寸固定，from_rgba 不会失败；失败时退回灰色兜底
    Icon::from_rgba(icon_rgba(state), ICON_SIZE, ICON_SIZE)
        .unwrap_or_else(|_| Icon::from_rgba(icon_rgba(IconState::Stopped), ICON_SIZE, ICON_SIZE).expect("兜底图标构建失败"))
}

fn read_launch_at_login(handle: &CoreHandle) -> bool {
    handle
        .store()
        .map(|s| s.get_preference(prefs::LAUNCH_AT_LOGIN))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icon_rgba_shape_and_colors() {
        for state in [
            IconState::Running,
            IconState::Stopped,
            IconState::Failed,
            IconState::Quitting,
        ] {
            let rgba = icon_rgba(state);
            assert_eq!(rgba.len(), (ICON_SIZE * ICON_SIZE * 4) as usize, "{:?}", state);
            // 中心像素不透明（圆点内）
            let center = ((ICON_SIZE / 2) * ICON_SIZE + ICON_SIZE / 2) as usize * 4;
            assert_eq!(rgba[center + 3], 255, "{:?} 中心应不透明", state);
            // 角落像素透明（圆点外）
            assert_eq!(rgba[3], 0, "{:?} 角落应透明", state);
        }
        // 四态颜色互不相同
        let colors: Vec<_> = [
            IconState::Running,
            IconState::Stopped,
            IconState::Failed,
            IconState::Quitting,
        ]
        .iter()
        .map(|s| {
            let rgba = icon_rgba(*s);
            let center = ((ICON_SIZE / 2) * ICON_SIZE + ICON_SIZE / 2) as usize * 4;
            (rgba[center], rgba[center + 1], rgba[center + 2])
        })
        .collect();
        for i in 0..colors.len() {
            for j in (i + 1)..colors.len() {
                assert_ne!(colors[i], colors[j], "状态 {} 与 {} 颜色应不同", i, j);
            }
        }
    }

    #[test]
    fn build_menu_titles_and_items() {
        let running = ServerStatus::Running("127.0.0.1:8080".into());
        let m_running = build_menu(&running, true);
        assert!(!m_running.id().0.is_empty(), "菜单应有稳定 id");

        let stopped = ServerStatus::Stopped;
        let m_stopped = build_menu(&stopped, false);
        assert_ne!(m_running.id().0, m_stopped.id().0, "每次构建应是新菜单实例");

        // 标题文案随状态变化
        assert!(title_text(&running).contains("运行中"));
        assert!(title_text(&running).contains("127.0.0.1:8080"));
        assert!(title_text(&stopped).contains("未启动"));
    }

    /// 无 SNI 宿主 / 无 D-Bus session bus 时 `init` 降级：图标缺席但
    /// 不 panic，状态刷新静默跳过。真实注册路径由
    /// `dbus-run-session` 下的集成测试覆盖（任务 2.5）。
    #[test]
    fn init_degrades_without_sni_host() {
        let mut tray = Tray::init();
        assert!(tray.degraded(), "无 SNI 宿主环境应降级为无图标");
        // 降级态下状态刷新不 panic
        tray.on_server_status(ServerStatus::Running("127.0.0.1:1".into()));
        tray.on_server_status(ServerStatus::Failed("bind".into()));
    }
}
