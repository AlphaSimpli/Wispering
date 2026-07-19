use std::{fs, path::PathBuf, process::Command, thread, time::Duration, env};

use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

fn config_file_path(app_handle: &AppHandle) -> Result<PathBuf, String> {
    let mut config_dir = app_handle
        .path()
        .config_dir()
        .map_err(|_| "Failed to locate app config directory".to_string())?;
    config_dir.push("wisper");
    fs::create_dir_all(&config_dir)
        .map_err(|e| format!("Failed to create app config directory: {}", e))?;
    config_dir.push("config.json");
    Ok(config_dir)
}

fn load_api_key(app_handle: &AppHandle) -> Result<String, String> {
    let file_path = config_file_path(app_handle)?;
    let contents = fs::read_to_string(&file_path)
        .map_err(|_| "No Groq API key found. Please add your API key in settings.".to_string())?;
    let config: serde_json::Value = serde_json::from_str(&contents)
        .map_err(|e| format!("Failed to parse config file: {}", e))?;
    config
        .get("groq_api_key")
        .and_then(|v| v.as_str())
        .map(|value| value.to_string())
        .ok_or_else(|| "No Groq API key found. Please add your API key in settings.".to_string())
}

#[tauri::command]
fn save_api_key(app_handle: AppHandle, api_key: String) -> Result<(), String> {
    if api_key.trim().is_empty() {
        return Err("API key cannot be empty".to_string());
    }

    let file_path = config_file_path(&app_handle)?;
    let config = serde_json::json!({ "groq_api_key": api_key });
    let contents = serde_json::to_string_pretty(&config)
        .map_err(|e| format!("Failed to serialize config: {}", e))?;
    fs::write(&file_path, contents)
        .map_err(|e| format!("Failed to save API key: {}", e))?;
    Ok(())
}

#[tauri::command]
fn get_api_key(app_handle: AppHandle) -> Result<String, String> {
    load_api_key(&app_handle)
}

/// Transcribe audio data using the Free Groq Whisper API
#[tauri::command]
async fn transcribe_audio(app_handle: AppHandle, audio_data: Vec<u8>) -> Result<String, String> {
    let api_key = load_api_key(&app_handle).map_err(|_| {
        "No Groq API key found. Please add your API key in settings.".to_string()
    })?;

    // Future alternative: use an environment variable if desired.
    // let api_key = env::var("GROQ_API_KEY")
    //     .map_err(|_| "Missing GROQ_API_KEY environment variable".to_string())?;

    println!("DEBUG: Groq API Key loaded from local config.");

    let client = reqwest::Client::new();

    let file_part = reqwest::multipart::Part::bytes(audio_data)
        .file_name("audio.webm")
        .mime_str("audio/webm")
        .map_err(|e| format!("Failed to set mime type: {}", e))?;

    let form = reqwest::multipart::Form::new()
        .part("file", file_part)
        .text("model", "whisper-large-v3-turbo");

    let response_result = client
        .post("https://api.groq.com/openai/v1/audio/transcriptions")
        .bearer_auth(&api_key)
        .multipart(form)
        .send()
        .await
        .map_err(|e| format!("Failed to send request to Groq: {}", e))?;

    let response_status = response_result.status();
    let response_text = response_result
        .text()
        .await
        .map_err(|e| format!("Failed to read Groq response body: {}", e))?;

    println!("DEBUG: Raw Groq Response (Status {}): {}", response_status, response_text);

    let response_json: serde_json::Value = serde_json::from_str(&response_text)
        .map_err(|e| format!("Failed to parse Groq JSON: {}. Raw response: {}", e, response_text))?;

    response_json
        .get("text")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| format!("No 'text' field in Groq response. Full response: {}", response_text))
}

#[tauri::command]
fn paste_text() -> Result<(), String> {
    println!("DEBUG: Starting paste automation");
    thread::sleep(Duration::from_millis(150));

    #[cfg(target_os = "macos")]
    {
        let status = Command::new("osascript")
            .args([
                "-e",
                "tell application \"System Events\" to keystroke \"v\" using command down",
            ])
            .status()
            .map_err(|e| format!("Failed to trigger paste: {}", e))?;

        if status.success() {
            println!("DEBUG: Paste action succeeded on macOS");
            Ok(())
        } else {
            eprintln!("DEBUG: Paste action failed on macOS with status: {:?}", status);
            Err(format!("Paste command exited with status: {:?}", status))
        }
    }

    #[cfg(target_os = "windows")]
    {
        let status = Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                "$wshell = New-Object -ComObject wscript.shell; $wshell.SendKeys('^v')",
            ])
            .status()
            .map_err(|e| format!("Failed to trigger paste: {}", e))?;

        if status.success() {
            println!("DEBUG: Paste action succeeded on Windows");
            Ok(())
        } else {
            eprintln!("DEBUG: Paste action failed on Windows with status: {:?}", status);
            Err(format!("Paste command exited with status: {:?}", status))
        }
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        Err("Paste automation is only supported on macOS and Windows".to_string())
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Automatically load variables from your root `.env` file during development
    let _ = dotenvy::dotenv();

    tauri::Builder::default()
        .setup(|app| {
            let main_window = app.get_webview_window("main").ok_or("main window not found")?;
            
            // Register target shortcut cleanly at the OS level once
            let shortcut_string = "Option+Space";
            match app.global_shortcut().register(shortcut_string) {
                Ok(()) => println!("🎉 SUCCESS: [{}] registered safely at OS level.", shortcut_string),
                Err(err) => {
                    eprintln!("⚠️ WARNING: OS blocked system registration for [{}]: {:?}", shortcut_string, err);
                    // Fallback to Ctrl+Shift+X if Option+Space is heavily locked down on this profile
                    let _ = app.global_shortcut().register("Ctrl+Shift+X");
                }
            }

            // Window configuration settings
            let _ = main_window.hide();
            let _ = main_window.set_decorations(false);
            let _ = main_window.set_resizable(true);
            let _ = main_window.set_always_on_top(true);
            let _ = main_window.set_skip_taskbar(true);
            let _ = main_window.set_title("Wispering");

            Ok(())
        })
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        // Native, single-pass Tauri v2 global event handler
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| {
                    if event.state() == ShortcutState::Pressed {
                        // Dynamically read string representation of the shortcut sequence
                        let desc = format!("{:?}", shortcut).to_lowercase();
                        
                        // Check against normal variants or fallback triggers safely
                        if desc.contains("option") && desc.contains("space") || desc.contains("alt") && desc.contains("space") || desc.contains("ctrl") && desc.contains("shift") && desc.contains("x") {
                            println!("🎯 EXECUTED: Global Hotkey triggered inside plugin context!");
                            if let Some(main_window) = app.get_webview_window("main") {
                                let _ = main_window.show();
                                let _ = main_window.set_focus();
                                let _ = main_window.emit("toggle-recording", ());
                            }
                        }
                    }
                })
                .build(),
        )
        .invoke_handler(tauri::generate_handler![greet, transcribe_audio, paste_text, save_api_key, get_api_key])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}