# Vigil: Development Utilities for Catalyst

Vigil is a spark (plugin) for Catalyst Framework that provides development utilities including hot reloading. It automatically detects changes to templates, stylesheets, and JavaScript files, triggering browser refreshes to make your development workflow smoother.

## Features

- **Comprehensive Hot Reload**: Watches for changes in:
  - Templates (`.tera`, `.html`)
  - Stylesheets (`.css`, `.scss`)
  - JavaScript/TypeScript (`.js`, `.ts`)
- **Multiple Directory Monitoring**: Watches several key directories:
  - `templates/` - For template files
  - `public/css/` - For CSS files
  - `public/js/` - For JavaScript files
  - `src/assets/` - For source assets (SCSS, TS, etc.)
- **Intelligent Debouncing**: Uses cooldown periods to prevent reload storms when multiple files are updated simultaneously
- **Zero Configuration**: Works out of the box with sensible defaults
- **Development Mode Only**: Only activates when Catalyst is in development mode
- **Cascading Configuration**: Settings can be adjusted through multiple methods with clear priority
- **Self-contained**: All necessary code is packaged within the spark - no external dependencies needed

## How It Works

1. In development mode (`environment = "dev"` in Catalyst.toml), Vigil injects a JavaScript file into your HTML pages via HTTP headers
2. This script connects to a WebSocket endpoint provided by Vigil
3. When files change, Vigil sends a message through the WebSocket with the changed file path and type
4. The browser automatically refreshes to show the updated content
5. Special error handling prevents console noise from missing scripts

## Configuration

Vigil supports a cascading configuration system with the following priority:

1. **Catalyst.toml** (highest priority) - Configure in the `[spark.vigil]` section
2. **Environment variables** - Use variables like `VIGIL_REFRESH_INTERVAL`
3. **manifest.toml** - Default values in the `[config.defaults]` section
4. **Hardcoded defaults** (lowest priority)

Example configuration in Catalyst.toml:

```toml
[spark.vigil]
# Make checks more frequent for more responsive feedback
refresh_interval = 200
# Prevent multiple reloads from happening too close together
cooldown_period = 250
```

Available configuration options:

| Option | Description | Default |
|--------|-------------|---------|
| `template_hot_reload` | Enable/disable hot reload functionality | `true` |
| `refresh_interval` | Milliseconds between checking for file changes | `400` |
| `cooldown_period` | Milliseconds to wait after reload before checking again | `100` |

## Usage

Vigil requires no user interaction - it's completely automatic:

1. Make changes to your templates, stylesheets, or JavaScript files
2. Save the file
3. Your browser will automatically refresh to show the changes

## Adding to Your Project

Vigil is included as a core spark in Catalyst. As long as you have your environment set to "dev" in Catalyst.toml, it will activate automatically.

```toml
[settings]
environment = "dev"
```
