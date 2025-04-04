use crate::cata_log;
use crate::services::sparks::registry::Spark;
use rocket::fairing::{Fairing, Info, Kind};
use rocket::http::{ContentType, Header};
use rocket::request::Request;
use rocket::response::content::RawJavaScript;
use rocket::response::Response;
use rocket::{get, routes, Build, Rocket};
use rocket_dyn_templates::Template;
use rocket_ws::Message;
use rocket_ws::WebSocket;
use serde_json::Value;
use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

// JS script for client-side hot reloading
const DEV_RELOAD_JS: &str = include_str!("dev-reload.js");

// Script injector that ensures our script is loaded
const SCRIPT_INJECTOR_JS: &str = r#"
// Vigil script injector
(function() {
    // This script is directly injected into the HTML
    // It looks for our script tag and creates it if needed
    if (!document.querySelector('script[src="/vigil/dev-reload.js"]')) {
        const script = document.createElement('script');
        script.src = '/vigil/dev-reload.js';
        script.setAttribute('data-hotreload', 'true');
        document.head.appendChild(script);
        console.log('[Vigil] Injected dev-reload script');
    }
})();
"#;

// Manifest for the spark
const MANIFEST_TOML: &str = include_str!("manifest.toml");

// Module for template watching in development mode
static LAST_MOD_TIME: AtomicU64 = AtomicU64::new(0);

// Global instance to expose settings
static VIGIL_INSTANCE: OnceLock<VigilSpark> = OnceLock::new();

#[derive(Clone)]
pub struct VigilSpark {
    environment: String,
    config: VigilConfig,
}

#[derive(Clone)]
struct VigilConfig {
    template_hot_reload: bool,
    refresh_interval: u32,
    cooldown_period: u32,
}

impl VigilSpark {
    fn new() -> Self {
        // Load environment setting from Catalyst.toml
        let environment = Self::get_environment();

        // Load config from manifest.toml and Catalyst.toml
        let config = Self::load_config();

        let instance = Self { environment, config };

        // Store the instance for global access
        let _ = VIGIL_INSTANCE.get_or_init(|| instance.clone());

        instance
    }

    // Parse manifest.toml and Catalyst.toml for configuration
    fn load_config() -> VigilConfig {
        use std::env;

        // Load and parse Catalyst.toml
        let toml_config = Self::parse_catalyst_toml();

        // Default configuration values
        let default_template_hot_reload = true;
        let default_refresh_interval = 1000;
        let default_cooldown_period = 3000;

        // Build config with cascading priority: Catalyst.toml -> env -> manifest.toml -> defaults
        let template_hot_reload = Self::get_config_bool(
            &toml_config,
            "template_hot_reload",
            "VIGIL_TEMPLATE_HOT_RELOAD",
            Self::get_manifest_bool("template_hot_reload", default_template_hot_reload),
        );

        let refresh_interval = Self::get_config_integer(&toml_config, "refresh_interval", "VIGIL_REFRESH_INTERVAL", Self::get_manifest_integer("refresh_interval", default_refresh_interval)) as u32;

        let cooldown_period = Self::get_config_integer(&toml_config, "cooldown_period", "VIGIL_COOLDOWN_PERIOD", Self::get_manifest_integer("cooldown_period", default_cooldown_period)) as u32;

        cata_log!(
            Info,
            format!(
                "Vigil config loaded: template_hot_reload={}, refresh_interval={}ms, cooldown_period={}ms",
                template_hot_reload, refresh_interval, cooldown_period
            )
        );

        VigilConfig {
            template_hot_reload,
            refresh_interval,
            cooldown_period,
        }
    }

    // Parse Catalyst.toml file
    fn parse_catalyst_toml() -> Option<toml::Value> {
        use std::fs;

        let config_path = "Catalyst.toml";
        let config_str = fs::read_to_string(config_path).unwrap_or_else(|_| {
            cata_log!(Warning, "Could not find Catalyst.toml, using default configuration");
            String::new()
        });

        if !config_str.is_empty() {
            match toml::from_str::<toml::Value>(&config_str) {
                Ok(config) => Some(config),
                Err(e) => {
                    cata_log!(Error, format!("Failed to parse Catalyst.toml: {}", e));
                    None
                }
            }
        } else {
            None
        }
    }

    // Get boolean value from manifest.toml config.defaults section
    fn get_manifest_bool(key: &str, default: bool) -> bool {
        if let Ok(manifest) = toml::from_str::<toml::Value>(MANIFEST_TOML) {
            // Check in config.defaults section
            if let Some(config) = manifest.get("config") {
                if let Some(defaults) = config.get("defaults") {
                    if let Some(value) = defaults.get(key) {
                        if let Some(bool_value) = value.as_bool() {
                            return bool_value;
                        }
                    }
                }
            }

            // Also check at root level for backward compatibility
            if let Some(value) = manifest.get(key) {
                if let Some(bool_value) = value.as_bool() {
                    return bool_value;
                }
            }
        }
        default
    }

    // Get integer value from manifest.toml config.defaults section
    fn get_manifest_integer(key: &str, default: i64) -> i64 {
        if let Ok(manifest) = toml::from_str::<toml::Value>(MANIFEST_TOML) {
            // Check in config.defaults section
            if let Some(config) = manifest.get("config") {
                if let Some(defaults) = config.get("defaults") {
                    if let Some(value) = defaults.get(key) {
                        if let Some(int_value) = value.as_integer() {
                            return int_value;
                        }
                    }
                }
            }

            // Also check at root level for backward compatibility
            if let Some(value) = manifest.get(key) {
                if let Some(int_value) = value.as_integer() {
                    return int_value;
                }
            }
        }
        default
    }

    // Helper to get a boolean config value with fallback to environment and default
    fn get_config_bool(toml_config: &Option<toml::Value>, key: &str, env_key: &str, default: bool) -> bool {
        use std::env;

        toml_config
            .as_ref()
            .and_then(|c| c.get("spark"))
            .and_then(|s| s.get("vigil"))
            .and_then(|p| p.get(key))
            .and_then(|v| v.as_bool())
            .unwrap_or_else(|| env::var(env_key).unwrap_or_else(|_| default.to_string()).parse().unwrap_or(default))
    }

    // Helper to get an integer config value with fallback to environment and default
    fn get_config_integer(toml_config: &Option<toml::Value>, key: &str, env_key: &str, default: i64) -> i64 {
        use std::env;

        toml_config
            .as_ref()
            .and_then(|c| c.get("spark"))
            .and_then(|s| s.get("vigil"))
            .and_then(|p| p.get(key))
            .and_then(|v| v.as_integer())
            .unwrap_or_else(|| env::var(env_key).unwrap_or_else(|_| default.to_string()).parse().unwrap_or(default))
    }

    // Helper to get the current environment from Catalyst.toml
    fn get_environment() -> String {
        let config_path = "Catalyst.toml";
        let config_str = fs::read_to_string(config_path).unwrap_or_default();

        if !config_str.is_empty() {
            if let Ok(toml) = toml::from_str::<toml::Value>(&config_str) {
                if let Some(settings) = toml.get("settings") {
                    if let Some(env) = settings.get("environment") {
                        if let Some(env_str) = env.as_str() {
                            return env_str.to_string();
                        }
                    }
                }
            }
        }

        // Default to production if not specified
        "prod".to_string()
    }

    // Check if any watched file has been modified
    fn check_template_changes() -> Option<String> {
        let mut latest_mod_time = 0;
        let mut changed_file = None;

        // Watch several directories for changes
        let watch_dirs = [
            Path::new("templates"),  // Template files
            Path::new("public/css"), // CSS files
            Path::new("public/js"),  // JavaScript files
            Path::new("src/assets"), // Source assets (SCSS, TS, etc.)
        ];

        // Walk each directory recursively
        for dir in watch_dirs.iter() {
            // Skip if directory doesn't exist
            if !dir.exists() {
                continue;
            }

            // Walk the directory recursively
            Self::walk_directory(dir, &mut latest_mod_time, &mut changed_file);
        }

        // Check if we have a new modification time that is greater than the last one we saw
        let last_time = LAST_MOD_TIME.load(Ordering::SeqCst);

        if latest_mod_time > last_time {
            // Update the atomic last mod time
            LAST_MOD_TIME.store(latest_mod_time, Ordering::SeqCst);

            // Print debug message
            // Determine file type from extension for more helpful logging
            let file_type = if let Some(file_path) = &changed_file {
                if file_path.ends_with(".tera") || file_path.ends_with(".html") {
                    "Template"
                } else if file_path.ends_with(".css") || file_path.ends_with(".scss") {
                    "Stylesheet"
                } else if file_path.ends_with(".js") || file_path.ends_with(".ts") {
                    "Script"
                } else {
                    "File"
                }
            } else {
                "File"
            };

            cata_log!(Debug, format!("{} change detected: {:?} at time {}", file_type, changed_file, latest_mod_time));

            // Return the changed file path
            changed_file
        } else {
            None
        }
    }

    // Helper function to recursively walk directories
    fn walk_directory(dir: &Path, latest_mod_time: &mut u64, changed_file: &mut Option<String>) {
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();

                if path.is_dir() {
                    // Recursively walk subdirectories
                    Self::walk_directory(&path, latest_mod_time, changed_file);
                } else if path.is_file() {
                    // Check for extension to determine if we should watch this file
                    if let Some(ext) = path.extension() {
                        let ext_str = ext.to_string_lossy().to_lowercase();
                        // Watch templates, stylesheets, and JavaScript files
                        if ext_str == "tera" || ext_str == "html" || ext_str == "css" || ext_str == "scss" || ext_str == "js" || ext_str == "ts" {
                            // Get file metadata and modification time
                            if let Ok(metadata) = fs::metadata(&path) {
                                if let Ok(mod_time) = metadata.modified() {
                                    if let Ok(seconds) = mod_time.duration_since(UNIX_EPOCH) {
                                        let seconds = seconds.as_secs();

                                        // Update latest mod time if newer
                                        if seconds > *latest_mod_time {
                                            *latest_mod_time = seconds;
                                            *changed_file = Some(path.to_string_lossy().to_string());
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

// WebSocket endpoint for template reloading
#[get("/ws/dev/reload")]
fn template_reload_websocket(ws: WebSocket) -> rocket_ws::Stream!['static] {
    // Set the initial timestamp to now instead of 0 to avoid fake changes
    let current_time = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();

    LAST_MOD_TIME.store(current_time, Ordering::SeqCst);

    rocket_ws::Stream! { ws =>
        // Send initial connection message but don't force reload
        yield Message::text("connected");

        // Keep track of errors
        let mut consecutive_errors = 0;
        let max_errors = 5;

        // Add a delay before starting checks to avoid initial duplicates
        rocket::tokio::time::sleep(std::time::Duration::from_millis(1000)).await;

        loop {
            // Add a try-catch for additional robustness
            let result = rocket::tokio::task::spawn_blocking(move || {
                VigilSpark::check_template_changes()
            }).await;

            match result {
                Ok(Some(changed_file)) => {
                    // Determine file type for more informative logging
                    let file_type = if changed_file.ends_with(".tera") || changed_file.ends_with(".html") {
                        "Template"
                    } else if changed_file.ends_with(".css") || changed_file.ends_with(".scss") {
                        "Stylesheet"
                    } else if changed_file.ends_with(".js") || changed_file.ends_with(".ts") {
                        "Script"
                    } else {
                        "File"
                    };

                    println!("{} changed: {}, sending reload signal", file_type, changed_file);
                    yield Message::text(format!("reload:{}", changed_file));

                    // Add a delay after sending a reload to prevent duplicate reloads
                    let cooldown = VIGIL_INSTANCE.get()
                        .map(|instance| instance.config.cooldown_period as u64)
                        .unwrap_or(3000);

                    rocket::tokio::time::sleep(std::time::Duration::from_millis(cooldown)).await;

                    consecutive_errors = 0;
                },
                Ok(None) => {
                    // No changes, just continue
                },
                Err(e) => {
                    // Error during check, log it
                    println!("Error checking for template changes: {:?}", e);
                    consecutive_errors += 1;

                    if consecutive_errors >= max_errors {
                        println!("Too many consecutive errors, breaking connection");
                        break;
                    }
                }
            }

            // Send a ping occasionally to keep the connection alive
            if consecutive_errors == 0 && rand::random::<u8>() < 10 {  // ~4% chance
                yield Message::text("ping");
            }

            // Sleep based on configured refresh interval before checking again
            let refresh_interval = VIGIL_INSTANCE.get()
                .map(|instance| instance.config.refresh_interval as u64)
                .unwrap_or(1000);

            rocket::tokio::time::sleep(std::time::Duration::from_millis(refresh_interval)).await;
        }
    }
}

// Endpoint to serve the JavaScript for hot reloading
#[get("/vigil/dev-reload.js")]
fn serve_dev_reload_js() -> RawJavaScript<&'static str> {
    RawJavaScript(DEV_RELOAD_JS)
}

// Endpoint to serve the script injector
#[get("/vigil/injector.js")]
fn serve_injector_js() -> RawJavaScript<&'static str> {
    RawJavaScript(SCRIPT_INJECTOR_JS)
}

// Endpoint to serve an HTML script tag with the script
#[get("/vigil/inject.js")]
fn serve_inject_script() -> RawJavaScript<String> {
    let script = r#"
    // Vigil Hot Reload Injector
    (function() {
        // Create a fetch request to get our own URL
        fetch(window.location.href, { method: 'HEAD' })
            .then(response => {
                // Check if Vigil is active from the headers
                const isActive = response.headers.get('X-Vigil-Active') === 'true';
                const scriptPath = response.headers.get('X-Vigil-Script-Path');
                
                if (isActive && scriptPath) {
                    console.log('[Vigil] Detected via header, loading from ' + scriptPath);
                    const script = document.createElement('script');
                    script.src = scriptPath;
                    script.setAttribute('data-hotreload', 'true');
                    document.head.appendChild(script);
                } else {
                    console.log('[Vigil] Not active or in production mode');
                }
            })
            .catch(err => {
                // Fail silently in production
                console.debug('[Vigil] Not in development mode or headers not available');
            });
    })();
    "#;

    RawJavaScript(script.to_string())
}

// Endpoint to serve the manifest.toml
#[get("/vigil/manifest.toml")]
fn serve_manifest() -> (ContentType, &'static str) {
    (ContentType::Plain, MANIFEST_TOML)
}

// Debug endpoint to verify integration
#[get("/vigil/status")]
fn serve_status() -> (ContentType, String) {
    let status = format!(
        r#"
    <html>
    <head>
        <title>Vigil Status</title>
    </head>
    <body>
        <h1>Vigil Development Tools</h1>
        <p>Status: Active</p>
        <p>Environment: {}</p>
        <p>Hot Reload: Enabled</p>
        <p>Last check: {}</p>
        <p>This page should have the auto-reload script injected.</p>
    </body>
    </html>
    "#,
        VIGIL_INSTANCE.get().map(|i| &i.environment).unwrap_or(&String::from("unknown")),
        LAST_MOD_TIME.load(Ordering::SeqCst)
    );

    (ContentType::HTML, status)
}

// Fairing to inject our script directly into HTML responses
struct ScriptInjectionFairing;

#[rocket::async_trait]
impl Fairing for ScriptInjectionFairing {
    fn info(&self) -> Info {
        Info {
            name: "Vigil Script Injector",
            kind: Kind::Response,
        }
    }

    async fn on_response<'r>(&self, _request: &'r Request<'_>, response: &mut Response<'r>) {
        // Only inject headers for HTML content
        if let Some(content_type) = response.content_type() {
            if content_type.is_html() {
                // Add HTTP headers for the JS snippet to detect
                response.set_header(Header::new("X-Vigil-Active", "true"));
                response.set_header(Header::new("X-Vigil-HotReload", "true"));
                response.set_header(Header::new("X-Vigil-Script-Path", "/vigil/dev-reload.js"));

                // Also add a CSP header to allow inline scripts
                let existing_csp = response.headers().get_one("Content-Security-Policy");
                if let Some(csp) = existing_csp {
                    // Append to existing CSP
                    response.set_header(Header::new("Content-Security-Policy", format!("{} script-src 'self' 'unsafe-inline';", csp)));
                } else {
                    // Set a new CSP
                    response.set_header(Header::new("Content-Security-Policy", "script-src 'self' 'unsafe-inline';"));
                }
            }
        }
    }
}

// Implementation of the Spark trait for the vigil module
impl Spark for VigilSpark {
    fn initialize(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        cata_log!(Info, format!("Vigil spark initialized in {} environment", self.environment));
        Ok(())
    }

    fn attach_to_rocket(&self, rocket: Rocket<Build>) -> Rocket<Build> {
        // Only attach template watching routes in development mode
        if self.environment == "dev" {
            cata_log!(Info, "Vigil: Development mode detected - enabling template hot reload");

            // These routes will be available in dev mode only
            rocket
                .mount("/", routes![template_reload_websocket, serve_dev_reload_js, serve_injector_js, serve_inject_script, serve_manifest, serve_status])
                .attach(ScriptInjectionFairing)
        } else {
            cata_log!(Info, "Vigil: Production mode detected - template hot reload disabled");
            rocket
        }
    }

    fn name(&self) -> &str {
        "vigil"
    }
}

// Export a function to create the spark
pub fn create_spark() -> Box<dyn crate::services::sparks::registry::Spark> {
    Box::new(VigilSpark::new())
}

