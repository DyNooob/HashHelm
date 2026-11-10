#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use base64::Engine as _;
use hashhelm::{
    app_log, containers, diagnostics, engine, import, platform, preflight, profile, scheduler,
    settings, startup, updater, verification,
};
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
    Drag(wry::DragDropEvent),
    Reply(Value),
}

fn main() {
    if let Some(code) = platform::guardian_entry() {
        std::process::exit(code);
    }
    if let Some(code) = verification::worker_entry() {
        std::process::exit(code);
    }
    if let Err(error) = desktop_main() {
        if std::env::args().any(|s| s == "--hashhelm-check") {
            eprintln!("{error}");
            app_log::event(
                "error",
                "startup_check_failed",
                json!({"message":error.to_string()}),
            );
        } else {
            startup::fatal(&error.to_string());
        }
        std::process::exit(1);
    }
}
fn desktop_main() -> Result<(), Box<dyn std::error::Error>> {
    let root = engine::root();
    if std::env::args().any(|s| s == "--hashhelm-platform-info" || s == "--forenhc-platform-info") {
        let data = platform::data_dir(&root).map_err(std::io::Error::other)?;
        println!(
            "{}",
            json!({"version":env!("CARGO_PKG_VERSION"),"build":updater::build_number(),"platform":platform::OS,"arch":platform::ARCH,"resources":root,"data":data,"engine":platform::engine_path(&root)})
        );
        return Ok(());
    }
    let data = startup::prepare_data(&root).map_err(std::io::Error::other)?;
    startup::initialize_log(&data).map_err(std::io::Error::other)?;
    std::fs::create_dir_all(&data)?;
    std::fs::create_dir_all(data.join("cache"))?;
    let mut runtime = startup::runtime(&root).map_err(std::io::Error::other)?;
    app_log::event("info", "runtime_selected", runtime.clone());
    let webview_version=wry::webview_version().map_err(|e|std::io::Error::other(format!("WebView2/WebKit 无法加载 / Cannot load web runtime: {e}. 请完整解压 offline 包；无需另装 VC++ 运行库。")))?;
    runtime["version"] = json!(webview_version);
    if std::env::args().any(|s| s == "--hashhelm-check") {
        println!("{}", startup::support_bundle(&root, &data, &runtime));
        return Ok(());
    }
    let startup_report = platform::setting("HASHHELM_STARTUP_REPORT", "FORENHC_STARTUP_REPORT")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute());
    let event_loop = EventLoopBuilder::<Msg>::with_user_event().build();
    app_log::event("info", "event_loop_ready", json!({}));
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
    app_log::event("info", "window_ready", json!({}));
    let ipc = proxy.clone();
    let drag_proxy = proxy.clone();
    // Do not reuse a browser environment started by older builds with different
    // security arguments. Existing profiles and user task data are preserved.
    let mut web_context = wry::WebContext::new(Some(data.join(if cfg!(windows) {
        "webview-secure"
    } else {
        "webview"
    })));
    let webview_builder = WebViewBuilder::new_with_web_context(&mut web_context)
        .with_html(
            include_str!("../ui/index.html")
                .replace("/* STYLES */", include_str!("../ui/style.css"))
                .replace("/* CORE */", include_str!("../ui/core.js"))
                .replace("/* I18N */", include_str!("../ui/i18n.js"))
                .replace("/* WORKBENCH */", include_str!("../ui/workbench.js"))
                .replace("/* UPDATES */", include_str!("../ui/updates.js"))
                .replace("/* TOOLS */", include_str!("../ui/tools.js"))
                .replace(
                    "<!-- LICENSE -->",
                    &include_str!("../LICENSE")
                        .replace('&', "&amp;")
                        .replace('<', "&lt;")
                        .replace('>', "&gt;"),
                )
                .replace(
                    "/* LOGO */",
                    &base64::engine::general_purpose::STANDARD
                        .encode(include_bytes!("../assets/branding/digiforensics.ico")),
                )
                .replace(
                    "<!-- WEBVIEW-LICENSE -->",
                    &include_str!("../assets/licenses/WebView2-Fixed-Version-License.txt")
                        .replace('&', "&amp;")
                        .replace('<', "&lt;")
                        .replace('>', "&gt;"),
                )
                .replace("/* APP */", include_str!("../ui/app.js")),
        )
        .with_drag_drop_handler(move |event| {
            let _ = drag_proxy.send_event(Msg::Drag(event));
            true
        })
        .with_navigation_handler(|url| {
            url == "about:blank"
                || url.starts_with("about:blank#")
                || url.starts_with("data:text/html")
        })
        .with_new_window_req_handler(|_, _| wry::NewWindowResponse::Deny)
        .with_ipc_handler(move |req| {
            let _ = ipc.send_event(Msg::Request(req.body().clone()));
        });
    #[cfg(windows)]
    let webview_builder = {
        use wry::WebViewBuilderExtWindows;
        // Preserve WebView2's security defaults. Wry's default arguments disable
        // SmartScreen and are unnecessary for our offline, navigation-locked UI.
        webview_builder.with_additional_browser_args("")
    };
    #[cfg(target_os = "linux")]
    let webview = {
        use tao::platform::unix::WindowExtUnix;
        use wry::WebViewBuilderExtUnix;
        let container = window
            .default_vbox()
            .ok_or("GTK window container is unavailable")?;
        webview_builder.build_gtk(container)?
    };
    #[cfg(not(target_os = "linux"))]
    let webview = webview_builder.build(&window)?;
    app_log::event("info", "webview_ready", json!({"runtime":runtime}));
    let running = Arc::new(AtomicBool::new(false));
    let cancel = Arc::new(AtomicBool::new(false));
    let mut frontend_error_shown = false;
    let engine_gate = Arc::new(AtomicBool::new(false));
    let queue_proxy = proxy.clone();
    let task_queue = scheduler::Scheduler::new(data.clone(), engine_gate.clone(), move |value| {
        let _ = queue_proxy.send_event(Msg::Reply(value));
    });
    let settings_proxy = proxy.clone();
    let settings = settings::Writer::new(data.clone(), move |message| {
        let _ = settings_proxy.send_event(Msg::Reply(json!({"type":"notice","message":message})));
    });
    let updates = updater::Controller::new(data.clone());
    event_loop.run(move |event, _, control| {
        *control = ControlFlow::Wait;
        match event {
            Event::WindowEvent { event: WindowEvent::CloseRequested, .. } => {
                let _ = webview.evaluate_script("window.requestClose()");
            }
            Event::UserEvent(Msg::Drag(event)) => {
                let (phase,paths,position)=match event {
                    wry::DragDropEvent::Enter{paths,position}=>("enter",paths,position),
                    wry::DragDropEvent::Over{position}=>("over",Vec::new(),position),
                    wry::DragDropEvent::Drop{paths,position}=>("drop",paths,position),
                    wry::DragDropEvent::Leave=>("leave",Vec::new(),(0,0)),_=>return,
                };
                // WebView2 emits physical client pixels; WKWebView/GTK emit logical coordinates.
                let scale=if cfg!(windows){window.scale_factor()}else{1.0};
                let items=if paths.len()<=20 {paths.iter().map(|path|json!({"path":path,"isFile":path.is_file(),"isDirectory":path.is_dir()})).collect::<Vec<_>>()}else{Vec::new()};
                let value=json!({"type":"file_drag","phase":phase,"x":position.0 as f64/scale,"y":position.1 as f64/scale,"count":paths.len(),"items":items});
                let _=webview.evaluate_script(&format!("window.receive({value})"));
            }
            Event::UserEvent(Msg::Reply(value)) => {
                app_log::response(&value);
                if value["type"]=="diagnosed" && let Ok(report)=serde_json::from_value::<diagnostics::Report>(value["report"].clone()) {task_queue.inventory(report);}
                if matches!(value["type"].as_str(),Some("diagnosed" | "diagnose_error")) {engine_gate.store(false,Ordering::SeqCst);}
                if matches!(value["type"].as_str(), Some("finished" | "error" | "imported" | "import_error" | "results" | "results_error" | "profile_done" | "profile_error" | "diagnosed" | "diagnose_error" | "preflight_done" | "preflight_error" | "file_verified" | "file_verify_error" | "plan_probe_done" | "plan_probe_error")) {
                    running.store(false, Ordering::SeqCst);
                }
                let _ = webview.evaluate_script(&format!("window.receive({value})"));
            }
            Event::UserEvent(Msg::Request(body)) => {
                let Ok(req) = serde_json::from_str::<Value>(&body) else { return; };
                let op = req["op"].as_str().unwrap_or("");
                app_log::operation(op);
                let reply = |v: Value| { let _ = proxy.send_event(Msg::Reply(v)); };
                match op {
                    "update_check" => {
                        let id=req["id"].as_u64().unwrap_or(0);
                        match updater::Options::new(req["channel"].as_str().unwrap_or("stable"), req["lang"].as_str().unwrap_or("zh")) {
                            Ok(options)=>{let proxy=proxy.clone();updates.check(options,id,move |v|{let _=proxy.send_event(Msg::Reply(v));});},
                            Err(code)=>reply(json!({"type":"update_error","id":id,"code":code})),
                        }
                    },
                    "update_download" => {
                        let id=req["id"].as_u64().unwrap_or(0);
                        if let Some(filename)=updates.filename() {
                            if let Some(path)=rfd::FileDialog::new().set_parent(&window).set_file_name(&filename).save_file() {
                                let proxy=proxy.clone();updates.download(path,id,move |v|{let _=proxy.send_event(Msg::Reply(v));});
                            } else {reply(json!({"type":"update_error","id":id,"code":"cancelled"}));}
                        } else {reply(json!({"type":"update_error","id":id,"code":"not_available"}));}
                    },
                    "update_cancel" => updates.cancel(),
                    "update_open_folder" => {
                        if let Some(path)=updates.saved_folder() && let Err(message)=platform::open_directory(&path) {
                            reply(json!({"type":"notice","message":message}));
                        }
                    },
                    "frontend_error"=>{
                        app_log::event("error","frontend_error",json!({"line":req["line"].as_u64(),"column":req["column"].as_u64(),"promise":req["kind"]=="promise"}));
                        if !frontend_error_shown {frontend_error_shown=true;startup::show_error("界面脚本发生异常 / The interface script failed. 请在日志与诊断中导出排障信息，或运行 Diagnose.cmd。");}
                    },
                    "application_log"=>reply(json!({"type":"application_log","log":app_log::view(),"runtime":runtime})),
                    "support_bundle"=>{
                        let bundle=startup::support_bundle(&root,&data,&runtime);
                        if let Some(path)=rfd::FileDialog::new().set_parent(&window).set_file_name("hashhelm-support.json").save_file(){
                            let message=match serde_json::to_vec_pretty(&bundle).map_err(|e|e.to_string()).and_then(|bytes|std::fs::write(path,bytes).map_err(|e|e.to_string())){Ok(())=>"诊断包已导出".to_owned(),Err(e)=>e};reply(json!({"type":"notice","message":message}));
                        }
                    },
                    "queue_get" => reply(json!({"type":"queue_state","queue":task_queue.snapshot()})),
                    "queue_submit" => match serde_json::from_value::<scheduler::Spec>(req["spec"].clone()) {
                        Ok(spec)=>task_queue.submit(spec),Err(error)=>reply(json!({"type":"queue_error","id":req["spec"]["id"],"message":error.to_string()})),
                    },
                    "queue_control" => {
                        let id=req["id"].as_str().unwrap_or("").to_owned();
                        match req["action"].as_str().unwrap_or("") {
                            "pause"=>task_queue.pause(true),"resume"=>task_queue.pause(false),"cancel"=>task_queue.cancel(id),
                            "cancel_all"=>task_queue.cancel_all(),"move"=>task_queue.move_job(id,req["delta"].as_i64().unwrap_or(0).clamp(-1,1) as i32),
                            "limit"=>task_queue.limit(req["limit"].as_u64().unwrap_or(1).clamp(1,4) as usize),_=>{},
                        }
                    }
                    "diagnose" => {
                        if running.swap(true,Ordering::SeqCst) {reply(json!({"type":"rejected","message":"请等待当前操作结束"}));return;}
                        if engine_gate.compare_exchange(false,true,Ordering::SeqCst,Ordering::SeqCst).is_err() {
                            running.store(false,Ordering::SeqCst);reply(json!({"type":"rejected","message":"请先暂停队列并等待运行任务结束，再检测设备"}));return;
                        }
                        cancel.store(false,Ordering::SeqCst);
                        let exe=PathBuf::from(req["engine"].as_str().unwrap_or(""));let data=data.clone();let cancel=cancel.clone();let proxy=proxy.clone();
                        std::thread::spawn(move || {
                            let value=match engine::inspect(&exe,&data,&cancel) {
                                Ok(report)=>{let _=settings::write_json(&data.join("device-report.json"),&json!(report));json!({"type":"diagnosed","report":report})},
                                Err(message)=>json!({"type":"diagnose_error","message":message}),
                            };let _=proxy.send_event(Msg::Reply(value));
                        });
                    }
                    "plan_probe" => {
                        if running.swap(true,Ordering::SeqCst) {reply(json!({"type":"rejected","message":"请等待当前操作结束"}));return;}
                        cancel.store(false,Ordering::SeqCst);
                        let exe=PathBuf::from(req["engine"].as_str().unwrap_or(""));let data=data.clone();let id=req["id"].clone();let cancel=cancel.clone();let proxy=proxy.clone();
                        std::thread::spawn(move || {
                            let result=engine::read_args(&req["args"]).and_then(|args|engine::plan_probe(&exe,&data,args,req["action"].as_str().unwrap_or(""),req["password"].as_str().unwrap_or(""),&cancel));
                            let value=match result{Ok(output)=>json!({"type":"plan_probe_done","id":id,"output":output}),Err(message)=>json!({"type":"plan_probe_error","id":id,"message":message})};let _=proxy.send_event(Msg::Reply(value));
                        });
                    }
                    "analyze_plan" => {
                        if running.swap(true,Ordering::SeqCst) {reply(json!({"type":"rejected","message":"请等待当前操作结束"}));return;}
                        cancel.store(false,Ordering::SeqCst);
                        let stages=req["stages"].as_array().cloned().unwrap_or_default();let id=req["id"].clone();let cancel=cancel.clone();let proxy=proxy.clone();
                        std::thread::spawn(move || {
                            let value=match preflight::analyze(&stages,&cancel) {Ok(stats)=>json!({"type":"preflight_done","id":id,"stats":stats}),Err(message)=>json!({"type":"preflight_error","id":id,"message":message})};
                            let _=proxy.send_event(Msg::Reply(value));
                        });
                    }
                    "import_template" => {
                        if let Some(path)=rfd::FileDialog::new().set_parent(&window).add_filter("HashHelm template",&["json"]).pick_file() {
                            let result=(||->Result<Value,String>{
                                if std::fs::metadata(&path).map_err(|e|e.to_string())?.len()>1024*1024 {return Err("模板文件不能超过 1 MiB".into());}
                                serde_json::from_slice(&std::fs::read(path).map_err(|e|e.to_string())?).map_err(|e|e.to_string())
                            })();
                            match result {Ok(template)=>reply(json!({"type":"template_file","template":template})),Err(message)=>reply(json!({"type":"notice","message":message}))}
                        }
                    }
                    "open_external" => {
                        if let Err(message)=platform::open_project_link(req["link"].as_str().unwrap_or("")) {
                            reply(json!({"type":"notice","message":message}));
                        }
                    }
                    "ui_ready" => {
                        app_log::event("info","frontend_ready",json!({}));
                        if let Some(path) = &startup_report {
                            use std::io::Write;
                            // An opt-in native CI handshake, never overwriting an existing file.
                            if let Ok(mut file) = std::fs::OpenOptions::new().write(true).create_new(true).open(path) {
                                let _ = write!(file,"{}",json!({"version":env!("CARGO_PKG_VERSION"),"build":updater::build_number(),"platform":platform::OS,"arch":platform::ARCH,"uiInitialized":true}));
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
                        if let Some(dest)=rfd::FileDialog::new().set_parent(&window).set_file_name("candidates.txt").save_file() {
                            let message=match std::fs::copy(source,dest){Ok(_)=>"字典已导出".to_owned(),Err(e)=>e.to_string()};reply(json!({"type":"notice","message":message}));
                        }
                    }
                    "verify_file" => {
                        if running.swap(true,Ordering::SeqCst){reply(json!({"type":"rejected","message":"请等待当前操作结束"}));return;}
                        let record_path=PathBuf::from(req["recordPath"].as_str().unwrap_or(""));
                        let record=match verification::load_record(&record_path,&data){Ok(record)=>record,Err(message)=>{reply(json!({"type":"file_verify_error","id":req["id"],"message":message}));return;}};
                        let destination=if req["export"]==true {
                            let filename=if record.format.starts_with("7z")||record.format.starts_with("RAR"){"decrypted-archive.zip".to_owned()}
                            else if record.format.starts_with("ZIP"){format!("decrypted-{}",record.member_name.as_deref().and_then(|s|s.rsplit(['/', '\\']).next()).unwrap_or("member.bin"))}
                            else{format!("decrypted-{}",record.source.file_name().unwrap_or_default().to_string_lossy())};
                            match rfd::FileDialog::new().set_parent(&window).set_file_name(&filename).save_file(){Some(path)=>Some(path),None=>{reply(json!({"type":"file_verify_error","id":req["id"],"message":"导出已取消"}));return;}}
                        }else{None};
                        let request=verification::Request{record,password_hex:req["passwordHex"].as_str().unwrap_or("").to_owned(),destination};
                        let id=req["id"].clone();let proxy=proxy.clone();let cancel=cancel.clone();let data=data.clone();cancel.store(false,Ordering::SeqCst);
                        std::thread::spawn(move||{
                            let value=match verification::run(request,&cancel){
                                Ok(receipt)=>{
                                    let dir=data.join("verification");let stamp=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos();
                                    let path=dir.join(format!("{stamp}.json"));
                                    let report=std::fs::create_dir_all(&dir).map_err(|e|e.to_string()).and_then(|_|settings::write_json(&path,&json!({"version":env!("CARGO_PKG_VERSION"),"receipt":receipt})));
                                    json!({"type":"file_verified","id":id,"receipt":receipt,"reportPath":if report.is_ok(){Some(path)}else{None},"reportError":report.err()})
                                },Err(message)=>json!({"type":"file_verify_error","id":id,"message":message})
                            };let _=proxy.send_event(Msg::Reply(value));
                        });
                    }
                    "results" => {
                        if running.swap(true, Ordering::SeqCst) { reply(json!({"type":"rejected","message":"请等待当前任务结束后查看结果"})); return; }
                        cancel.store(false, Ordering::SeqCst);
                        let exe=PathBuf::from(req["engine"].as_str().unwrap_or(""));
                        let target=PathBuf::from(req["target"].as_str().unwrap_or(""));
                        let mode=req["mode"].as_str().and_then(|s|s.parse::<u32>().ok());
                        let pot=data.join("recovered.potfile");let request_id=req["id"].clone();let proxy=proxy.clone();let cancel=cancel.clone();
                        std::thread::spawn(move || {
                            let value=match mode.ok_or_else(||"无效的哈希类型".to_owned()).and_then(|m|engine::results_with_cancel(&exe,&target,m,&pot,&cancel)) {
                                Ok(entries)=>json!({"type":"results","id":request_id,"entries":entries}),
                                Err(message)=>json!({"type":"results_error","id":request_id,"message":message})
                            };
                            let _=proxy.send_event(Msg::Reply(value));
                        });
                    }
                    "mask_file" => {
                        let path=PathBuf::from(req["path"].as_str().unwrap_or(""));
                        let result=(||->Result<String,String>{
                            let meta=std::fs::metadata(&path).map_err(|e|e.to_string())?;
                            if !meta.is_file()||meta.len()>1024*1024{return Err("掩码文件需为 1 MiB 以内的文本".into());}
                            std::fs::read_to_string(path).map_err(|_|"掩码文件需使用 UTF-8 编码".into())
                        })();
                        reply(match result {Ok(text)=>json!({"type":"mask_file","id":req["id"],"text":text}),Err(message)=>json!({"type":"mask_file_error","id":req["id"],"message":message})});
                    },
                    "import" => {
                        if running.swap(true, Ordering::SeqCst) { reply(json!({"type":"rejected","message":"已有操作正在运行，请等待结束"})); return; }
                        cancel.store(false, Ordering::SeqCst);
                        let path=PathBuf::from(req["path"].as_str().unwrap_or(""));
                        let member=req["member"].as_u64().map(|v|v as usize);
                        let container=req["container"].clone();
                        let id=req["id"].clone();let data=data.clone();let proxy=proxy.clone();let cancel=cancel.clone();
                        std::thread::spawn(move || {
                            let result=(|| -> Result<Value,String> {
                                let spec=if container.is_null(){None}else{Some(serde_json::from_value::<containers::Spec>(container).map_err(|_|"容器参数无效")?)};
                                let before=if let Some(spec)=&spec{verification::record_container(&path,spec.clone(),&cancel)?}else{verification::record(&path,member,String::new(),&cancel)?};
                                let extracted=if let Some(spec)=&spec{containers::extract(&path,spec)?}else{import::extract(&path,member)?};
                                let mut record=if let Some(spec)=&spec{verification::record_container(&path,spec.clone(),&cancel)?}else{verification::record(&path,extracted.selected,extracted.format.clone(),&cancel)?};
                                if before.sha256!=record.sha256||before.size!=record.size||before.container!=record.container{return Err("分析期间原文件或密钥文件发生改变，请重新导入".into());}
                                record.member=extracted.selected;
                                record.member_name=extracted.selected.and_then(|index|extracted.members.iter().find(|m|m.index==index)).map(|m|m.name.clone());
                                if cancel.load(Ordering::SeqCst) { return Err("导入已取消".into()); }
                                let stamp=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_err(|e|e.to_string())?.as_nanos();
                                let dir=data.join("imports").join(stamp.to_string());std::fs::create_dir_all(&dir).map_err(|e|e.to_string())?;
                                let hash_path=dir.join("target.hash");std::fs::write(&hash_path,format!("{}\n",extracted.hash)).map_err(|e|e.to_string())?;
                                let result=json!({"type":"imported","id":id,"source":path,"hashPath":hash_path,"mode":extracted.mode,"format":extracted.format,"note":extracted.note,"members":extracted.members,"selected":extracted.selected,"recordPath":dir.join("source.json"),"modes":record.container.as_ref().map(|r|r.spec.modes()),"container":record.container,"verification":record});
                                std::fs::write(dir.join("source.json"),result.to_string()).map_err(|e|e.to_string())?;Ok(result)
                            })();
                            let _=proxy.send_event(Msg::Reply(result.unwrap_or_else(|message|json!({"type":"import_error","id":id,"message":message}))));
                        });
                    }
                    "init" => {
                        let config = settings::load(&data);
                        let engine=config.as_ref().and_then(|c|c["engine"].as_str().or_else(||c["fields"]["engine"].as_str())).map(PathBuf::from).filter(|p|platform::executable(p)).unwrap_or_else(||engine::engine_path(&root));
                        let device_report=std::fs::read(data.join("device-report.json")).ok().and_then(|bytes|serde_json::from_slice::<Value>(&bytes).ok());
                        let light_stats=preflight::analyze(&[json!({"config":{"rules":root.join("assets/rules/light.rule").to_string_lossy()}})],&AtomicBool::new(false)).ok();
                        reply(json!({"type":"init", "engineExists":platform::executable(&engine), "engine":engine, "data":data, "resources":root,"platform":platform::OS,"arch":platform::ARCH,"buildLabel":platform::display_name(),"version":env!("CARGO_PKG_VERSION"),"build":updater::build_number(),"config":config,"dictionaries":profile::dictionary_catalog(&root),"deviceReport":device_report,"inputStats":light_stats,"queue":task_queue.snapshot(),"runtime":runtime,"dataFallback":platform::data_dir(&root).is_ok_and(|preferred|preferred!=data)}));
                    }
                    "checkpoints" => {
                        let paths=req["paths"].as_array().cloned().unwrap_or_default();
                        let id=req["id"].clone();let proxy=proxy.clone();
                        std::thread::spawn(move || {let items=paths.iter().filter_map(|p|p.as_str()).take(1000).map(|p|{let path=std::path::Path::new(p);match hashhelm::sessions::inspect(path){Ok(info)=>json!({"path":p,"exists":true,"info":info}),Err(message)=>json!({"path":p,"exists":false,"error":if path.exists(){message}else{String::new()}})}}).collect::<Vec<_>>();let _=proxy.send_event(Msg::Reply(json!({"type":"checkpoints","id":id,"items":items})));});
                    }
                    "read_log" => {
                        let path=PathBuf::from(req["path"].as_str().unwrap_or(""));
                        match engine::read_log(&path,&data) {
                            Ok(text)=>reply(json!({"type":"log","id":req["id"],"text":text})),
                            Err(message)=>reply(json!({"type":"log","id":req["id"],"text":message})),
                        }
                    }
                    "save" => { if let Err(message) = settings.save(req["config"].clone()) { reply(json!({"type":"notice","message":message})); } }
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
                        let path = if kind == "folder" { rfd::FileDialog::new().set_parent(&window).pick_folder() }
                            else if kind == "save" { rfd::FileDialog::new().set_parent(&window).set_file_name("results.txt").save_file() }
                            else { rfd::FileDialog::new().set_parent(&window).pick_file() };
                        if let Some(path) = path { reply(json!({"type":"picked","field":req["field"],"path":path})); }
                    }
                    "export" => {
                        let progress = req["kind"].as_str() == Some("progress");
                        let filename = match req["kind"].as_str() {Some("progress")=>"hashhelm-progress.json",Some("diagnostic")=>"hashhelm-diagnostics.json",Some("template")=>"hashhelm-template.json",Some("verification")=>"hashhelm-verification.json",_=>"hashhelm-log.txt"};
                        if let Some(path) = rfd::FileDialog::new().set_parent(&window).set_file_name(filename).save_file() {
                            let result = std::fs::write(path, req["text"].as_str().unwrap_or(""));
                            let message = match result { Ok(()) => if progress { "进度快照已导出" } else { "文件已导出" }.to_string(), Err(e) => e.to_string() };
                            reply(json!({"type":"notice","message":message}));
                        }
                    }
                    "stop" => { cancel.store(true, Ordering::SeqCst); }
                    "close" => { cancel.store(true, Ordering::SeqCst); task_queue.cancel_all(); updates.cancel(); if !running.load(Ordering::SeqCst) && !task_queue.busy() && !updates.downloading() { match settings.flush() { Ok(())=>*control = ControlFlow::Exit, Err(message)=>reply(json!({"type":"notice","message":message})) } } else { reply(json!({"type":"notice","message":"正在结束子进程，结束后可关闭窗口"})); } }
                    "run" => {
                        if running.swap(true, Ordering::SeqCst) { reply(json!({"type":"rejected","message":"已有任务正在运行，请等待结束"})); return; }
                        if engine_gate.compare_exchange(false,true,Ordering::SeqCst,Ordering::SeqCst).is_err() {running.store(false,Ordering::SeqCst);reply(json!({"type":"error","message":"队列正在使用计算设备，请先暂停派发并结束运行任务"}));return;}
                        cancel.store(false, Ordering::SeqCst);
                        let args = match engine::read_args(&req["args"]) { Ok(a)=>a, Err(e)=>{running.store(false, Ordering::SeqCst);engine_gate.store(false,Ordering::SeqCst);reply(json!({"type":"error","message":e})); return;} };
                        let exe = req["engine"].as_str().map(PathBuf::from).unwrap_or_else(|| engine::engine_path(&root));
                        let proxy = proxy.clone(); let cancel = cancel.clone(); let data = data.clone();let gate=engine_gate.clone();
                        std::thread::spawn(move || {
                            engine::run(exe, args, data, cancel, |v| { let _ = proxy.send_event(Msg::Reply(v)); });
                            gate.store(false,Ordering::SeqCst);
                        });
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    });
}
