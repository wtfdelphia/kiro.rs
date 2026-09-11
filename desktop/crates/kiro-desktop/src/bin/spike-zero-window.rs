//! Spike（设计文档 §6.5 之一）：零窗口时 GPUI 应用循环是否存活
//!
//! 流程：开窗 → 每秒心跳 → 第 3 秒销毁窗口 → 继续心跳到第 8 秒 → 退出。
//! 若销毁后心跳继续，说明应用循环在零窗口下存活；若 `run()` 提前返回，
//! 说明零窗口触发自动退出（常驻需用关窗拦截兜底，语义不变）。
//!
//! 运行：`xvfb-run -a cargo run --release --bin spike-zero-window`
//! 本 bin 仅用于 spike 取证，不属于应用交付物。

use gpui_kit::component::Root;
use gpui_kit::prelude::*;
use gpui_kit::*;

struct SpikeView;

impl Render for SpikeView {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child("zero-window spike")
    }
}

fn main() {
    let app = gpui_kit::application().with_assets(assets::AllAssets);
    app.run(|cx| {
        gpui_kit::init(cx);

        cx.spawn(async move |cx| {
            let handle = cx
                .open_window(WindowOptions::default(), |window, cx| {
                    let view = cx.new(|_| SpikeView);
                    cx.new(|cx| Root::new(view, window, cx))
                })
                .expect("spike 窗口创建失败");
            eprintln!("[spike] 窗口已打开: {}", handle.window_id().as_u64());

            let mut ticks = 0u32;
            loop {
                cx.background_executor()
                    .timer(std::time::Duration::from_secs(1))
                    .await;
                ticks += 1;
                eprintln!("[spike] tick {}", ticks);

                if ticks == 3 {
                    let removed = handle
                        .update(cx, |_, window, _| window.remove_window())
                        .is_ok();
                    eprintln!("[spike] tick 3: 窗口已销毁 (remove_window ok={})", removed);
                }
                if ticks == 8 {
                    eprintln!("[spike] tick 8: 销毁后应用循环仍存活（零窗口存活确认）");
                    cx.update(|app| app.quit());
                    break;
                }
            }
        })
        .detach();
    });
    eprintln!("[spike] run() 返回，应用已退出");
}
