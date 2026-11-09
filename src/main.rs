#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use hashhelm::{engine, import, platform, profile};
use base64::Engine as _;
use serde_json::{Value, json};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
use tao::{
    dpi::LogicalSize,
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoopBuilder},
    window::WindowBuilder,
};
use wry::WebViewBuilder;

#[derive(Debug, Clone)]
enum Msg {
    Request(String),
    Reply(Value),
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if let Some(code) = platform::guardian_entry() { std::process::exit(code); }
    let root = engine::root();
    let data = platform::data_dir(&root).map_err(std::io::Error::other)?;
    if std::env::args().any(|s| s == "--hashhelm-platform-info" || s == "--forenhc-platform-info") {
        println!("{}", json!({"version":env!("CARGO_PKG_VERSION"),"platform":platform::OS,"arch":platform::ARCH,"resources":root,"data":data,"engine":platform::engine_path(&root)}));
        return Ok(());
    }
    std::fs::create_dir_all(&data)?;
    std::fs::create_dir_all(data.join("cache"))?;
    let startup_report = platform::setting("HASHHELM_STARTUP_REPORT", "FORENHC_STARTUP_REPORT").map(PathBuf::from).filter(|p|p.is_absolute());
    let event_loop = EventLoopBuilder::<Msg>::with_user_event().build();
    let proxy = event_loop.create_proxy();
    let window = WindowBuilder::new()
        .with_title("HashHelm · DigiForensics")
        .with_inner_size(LogicalSize::new(1240.0, 860.0))
        .with_min_inner_size(LogicalSize::new(980.0, 680.0))
        .with_window_icon(
            tao::window::Icon::from_rgba(
                include_bytes!("../assets/branding/window-icon.rgba").to_vec(),
                64,
                64,
            )
            .ok(),
        )
        .build(&event_loop)?;
    let ipc = proxy.clone();
    let mut web_context = wry::WebContext::new(Some(data.join("webview")));
    let webview_builder = WebViewBuilder::new_with_web_context(&mut web_context)
        .with_html(
            include_str!("../ui/index.html")
                .replace("/* STYLES */", include_str!("../ui/style.css"))
                .replace("/* CORE */", include_str!("../ui/core.js"))
                .replace("/* I18N */", include_str!("../ui/i18n.js"))
                .replace("<!-- LICENSE -->", &include_str!("../LICENSE").replace('&',"&amp;").replace('<',"&lt;").replace('>',"&gt;"))
                .replace(
                    "/* LOGO */",
                    &base64::engine::general_purpose::STANDARD
                        .encode(include_bytes!("../assets/branding/digiforensics.png")),
                )
                .replace("/* APP */", include_str!("../ui/app.js")),
        )
        .with_navigation_handler(|url| {
            url == "about:blank"
                || url.starts_with("about:blank#")
                || url.starts_with("data:text/html")
        })
        .with_new_window_req_handler(|_, _| wry::NewWindowResponse::Deny)
        .with_ipc_handler(move |req| {
            let _ = ipc.send_event(Msg::Request(req.body().clone()));
        });
    #[cfg(target_os = "linux")]
    let webview = {
        use tao::platform::unix::WindowExtUnix;
        use wry::WebViewBuilderExtUnix;
        let container = window.default_vbox().ok_or("GTK window container is unavailable")?;
        webview_builder.build_gtk(container)?
    };
    #[cfg(not(target_os = "linux"))]
    let webview = webview_builder.build(&window)?;
    let running = Arc::new(AtomicBool::new(false));
    let cancel = Arc::new(AtomicBool::new(false));
    event_loop.run(move |event, _, control| {
        *control = ControlFlow::Wait;
        match event {
            Event::WindowEvent { event: WindowEvent::CloseRequested, .. } => {
                let _ = webview.evaluate_script("window.requestClose()");
            }
            Event::UserEvent(Msg::Reply(value)) => {
                if matches!(value["type"].as_str(), Some("finished" | "error" | "imported" | "import_error" | "results" | "results_error" | "profile_done" | "profile_error")) {
                    running.store(false, Ordering::SeqCst);
                }
                let _ = webview.evaluate_script(&format!("window.receive({value})"));
            }
            Event::UserEvent(Msg::Request(body)) => {
                let Ok(req) = serde_json::from_str::<Value>(&body) else { return; };
                let op = req["op"].as_str().unwrap_or("");
                let reply = |v: Value| { let _ = proxy.send_event(Msg::Reply(v)); };
                match op {
                    "open_external" => {
                        if let Err(message)=platform::open_project_link(req["link"].as_str().unwrap_or("")) {
                            reply(json!({"type":"notice","message":message}));
                        }
                    }
                    "ui_ready" => {
                        if let Some(path) = &startup_report {
                            use std::io::Write;
                            // An opt-in native CI handshake, never overwriting an existing file.
                            if let Ok(mut file) = std::fs::OpenOptions::new().write(true).create_new(true).open(path) {
                                let _ = write!(file,"{}",json!({"version":env!("CARGO_PKG_VERSION"),"platform":platform::OS,"arch":platform::ARCH,"uiInitialized":true}));
                            }
                        }
                    }
                    "appearance" => {
                        window.set_theme(Some(if req["theme"]=="dark" {tao::window::Theme::Dark} else {tao::window::Theme::Light}));
                    }
                    "profile" => {
                        if running.swap(true, Ordering::SeqCst) { reply(json!({"type":"rejected","message":"请等待当前任务结束"})); return; }
                        let input=req["profile"].clone();let request_id=req["id"].clone();let data=data.clone();let proxy=proxy.clone();
                        std::thread::spawn(move || {
                            let result=serde_json::from_value::<profile::Profile>(input).map_err(|e|e.to_string()).and_then(|p|profile::save(&p,&data));
                            let message=match result {Ok(generation)=>json!({"type":"profile_done","id":request_id,"generation":generation}),Err(message)=>json!({"type":"profile_error","id":request_id,"message":message})};
                            let _=proxy.send_event(Msg::Reply(message));
                        });
                    }
                    "export_dictionary" => {
                        let source=PathBuf::from(req["path"].as_str().unwrap_or(""));
                        if let Some(dest)=rfd::FileDialog::new().set_file_name("candidates.txt").save_file() {
                            let message=match std::fs::copy(source,dest){Ok(_)=>"字典已导出".to_owned(),Err(e)=>e.to_string()};reply(json!({"type":"notice","message":message}));
                        }
                    }
                    "results" => {
                        if running.swap(true, Ordering::SeqCst) { reply(json!({"type":"rejected","message":"请等待当前任务结束后查看结果"})); return; }
                        let exe=PathBuf::from(req["engine"].as_str().unwrap_or(""));
                        let target=PathBuf::from(req["target"].as_str().unwrap_or(""));
                        let mode=req["mode"].as_str().and_then(|s|s.parse::<u32>().ok());
                        let pot=data.join("recovered.potfile");let request_id=req["id"].clone();let proxy=proxy.clone();
                        std::thread::spawn(move || {
                            let value=match mode.ok_or_else(||"无效的哈希类型".to_owned()).and_then(|m|engine::results(&exe,&target,m,&pot)) {
                                Ok(entries)=>json!({"type":"results","id":request_id,"entries":entries}),
                                Err(message)=>json!({"type":"results_error","id":request_id,"message":message})
                            };
                            let _=proxy.send_event(Msg::Reply(value));
                        });
                    }
                    "import" => {
                        if running.swap(true, Ordering::SeqCst) { reply(json!({"type":"rejected","message":"已有操作正在运行，请等待结束"})); return; }
                        cancel.store(false, Ordering::SeqCst);
                        let path=PathBuf::from(req["path"].as_str().unwrap_or(""));
                        let member=req["member"].as_u64().map(|v|v as usize);
                        let id=req["id"].clone();let data=data.clone();let proxy=proxy.clone();let cancel=cancel.clone();
                        std::thread::spawn(move || {
                            let result=(|| -> Result<Value,String> {
                                let extracted=import::extract(&path,member)?;
                                if cancel.load(Ordering::SeqCst) { return Err("导入已取消".into()); }
                                let stamp=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_err(|e|e.to_string())?.as_nanos();
                                let dir=data.join("imports").join(stamp.to_string());std::fs::create_dir_all(&dir).map_err(|e|e.to_string())?;
                                let hash_path=dir.join("target.hash");std::fs::write(&hash_path,format!("{}\n",extracted.hash)).map_err(|e|e.to_string())?;
                                let result=json!({"type":"imported","id":id,"source":path,"hashPath":hash_path,"mode":extracted.mode,"format":extracted.format,"note":extracted.note,"members":extracted.members,"selected":extracted.selected});
                                std::fs::write(dir.join("source.json"),result.to_string()).map_err(|e|e.to_string())?;Ok(result)
                            })();
                            let _=proxy.send_event(Msg::Reply(result.unwrap_or_else(|message|json!({"type":"import_error","id":id,"message":message}))));
                        });
                    }
                    "init" => {
                        let config = std::fs::read_to_string(data.join("settings.json")).ok().and_then(|s| serde_json::from_str::<Value>(&s).ok());
                        let engine=config.as_ref().and_then(|c|c["engine"].as_str().or_else(||c["fields"]["engine"].as_str())).map(PathBuf::from).filter(|p|platform::executable(p)).unwrap_or_else(||engine::engine_path(&root));
                        reply(json!({"type":"init", "engineExists":platform::executable(&engine), "engine":engine, "data":data, "resources":root,"platform":platform::OS,"arch":platform::ARCH,"buildLabel":platform::display_name(),"config":config,"dictionaries":profile::dictionary_catalog(&root)}));
                    }
                    "checkpoints" => {
                        let paths=req["paths"].as_array().cloned().unwrap_or_default();
                        reply(json!({"type":"checkpoints","items":paths.iter().filter_map(|p|p.as_str()).take(1000).map(|p|json!({"path":p,"exists":std::path::Path::new(p).is_file()})).collect::<Vec<_>>()}));
                    }
                    "read_log" => {
                        let path=PathBuf::from(req["path"].as_str().unwrap_or(""));
                        match engine::read_log(&path,&data) {
                            Ok(text)=>reply(json!({"type":"log","id":req["id"],"text":text})),
                            Err(message)=>reply(json!({"type":"log","id":req["id"],"text":message})),
                        }
                    }
                    "save" => { if let Err(e) = std::fs::write(data.join("settings.json"), req["config"].to_string()) { reply(json!({"type":"notice","message":e.to_string()})); } }
                    "demo" => {
                        let result = std::fs::write(data.join("demo.hash"), "8743b52063cd84097a65d1633f5c74f5\n")
                            .and_then(|_| std::fs::write(data.join("demo.dict"), "incorrect\nhashcat\nhashhelm\n"));
                        match result { Ok(()) => reply(json!({"type":"demo","target":data.join("demo.hash"),"wordlist":data.join("demo.dict")})), Err(e) => reply(json!({"type":"notice","message":e.to_string()})) }
                    }
                    "candidates" => {
                        let text=req["text"].as_str().unwrap_or("");
                        if text.lines().all(|l| l.is_empty()) { reply(json!({"type":"notice","message":"请先输入候选密码"})); }
                        else { let stamp=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos();let path=data.join(format!("candidates-{stamp}.txt"));match std::fs::write(&path, text) { Ok(()) => reply(json!({"type":"candidates","path":path})), Err(e) => reply(json!({"type":"notice","message":e.to_string()})) } }
                    }
                    "pick" => {
                        let kind = req["kind"].as_str().unwrap_or("file");
                        let path = if kind == "folder" { rfd::FileDialog::new().pick_folder() }
                            else if kind == "save" { rfd::FileDialog::new().set_file_name("results.txt").save_file() }
                            else { rfd::FileDialog::new().pick_file() };
                        if let Some(path) = path { reply(json!({"type":"picked","field":req["field"],"path":path})); }
                    }
                    "export" => {
                        let progress = req["kind"].as_str() == Some("progress");
                        let filename = if progress { "hashhelm-progress.json" } else { "hashhelm-log.txt" };
                        if let Some(path) = rfd::FileDialog::new().set_file_name(filename).save_file() {
                            let result = std::fs::write(path, req["text"].as_str().unwrap_or(""));
                            let message = match result { Ok(()) => if progress { "进度快照已导出" } else { "日志已导出" }.to_string(), Err(e) => e.to_string() };
                            reply(json!({"type":"notice","message":message}));
                        }
                    }
                    "stop" => { cancel.store(true, Ordering::SeqCst); }
                    "close" => { cancel.store(true, Ordering::SeqCst); if !running.load(Ordering::SeqCst) { *control = ControlFlow::Exit; } else { reply(json!({"type":"notice","message":"正在结束子进程，结束后可关闭窗口"})); } }
                    "run" => {
                        if running.swap(true, Ordering::SeqCst) { reply(json!({"type":"rejected","message":"已有任务正在运行，请等待结束"})); return; }
                        cancel.store(false, Ordering::SeqCst);
                        let args = match engine::read_args(&req["args"]) { Ok(a)=>a, Err(e)=>{running.store(false, Ordering::SeqCst);reply(json!({"type":"error","message":e})); return;} };
                        let exe = req["engine"].as_str().map(PathBuf::from).unwrap_or_else(|| engine::engine_path(&root));
                        let proxy = proxy.clone(); let cancel = cancel.clone(); let data = data.clone();
                        std::thread::spawn(move || {
                            engine::run(exe, args, data, cancel, |v| { let _ = proxy.send_event(Msg::Reply(v)); });
                        });
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    });
}
