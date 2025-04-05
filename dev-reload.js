// Vigil template hot reload client
(function() {
    console.log('[Vigil] Script loaded successfully!');
    
    // Check if the X-Vigil-HotReload header exists to enable hot reload
    const isDevMode = document.currentScript?.getAttribute('data-hotreload') === 'true' ||
                      document.querySelector('meta[name="x-vigil-hotreload"]')?.getAttribute('content') === 'true' ||
                      true; // Force dev mode for testing
    
    if (!isDevMode) {
        // Script loaded but not in dev mode
        console.debug('[Vigil] Development mode not detected, hot reload disabled');
        return;
    }
    
    // Handle errors from missing scripts quietly
    window.addEventListener('error', function(event) {
        // Ignore missing scripts for app.min.js (referenced in the templates but might not exist)
        if (event.target && event.target.tagName === 'SCRIPT') {
            const src = event.target.src || '';
            if (src.includes('/app.min.js') || src.includes('/app/app.min.js')) {
                console.warn('[Vigil] Ignoring error for missing script:', src);
                event.preventDefault();
                return true;
            }
        }
    }, true);
    
    console.log('[Vigil] Development mode detected, connecting to reload websocket at ws://' + window.location.host + '/ws/dev/reload');
    
    // Track reload status
    window.lastTemplateReload = 0;
    let reconnectTimer = null;
    let reconnectAttempts = 0;
    let isReconnecting = false;
    
    function connectWebSocket() {
        // Clear any existing reconnect timer
        if (reconnectTimer) {
            clearTimeout(reconnectTimer);
            reconnectTimer = null;
        }
        
        // Create a new WebSocket connection
        const ws = new WebSocket(`ws://${window.location.host}/ws/dev/reload`);
        
        // Connection opened
        ws.addEventListener('open', (event) => {
            console.log('[Vigil] Connected to template reload websocket');
            reconnectAttempts = 0;
            isReconnecting = false;
        });
        
        // Track last ping time for connection health monitoring
        let lastPingTime = Date.now();
        let connectionId = null;
        
        // Listen for messages from the server
        ws.addEventListener('message', (event) => {
            const message = event.data;
            
            if (message.startsWith('reload:')) {
                // Extract the changed file path for logging
                const filePath = message.substring(7);
                
                // Determine file type from extension
                let fileType = "file";
                if (filePath.endsWith(".tera") || filePath.endsWith(".html")) {
                    fileType = "template";
                } else if (filePath.endsWith(".css") || filePath.endsWith(".scss")) {
                    fileType = "stylesheet";
                } else if (filePath.endsWith(".js") || filePath.endsWith(".ts")) {
                    fileType = "script";
                }
                
                console.log(`[Vigil] ${fileType.charAt(0).toUpperCase() + fileType.slice(1)} changed: ${filePath}, reloading page...`);
                
                // Track last reload to prevent duplicates
                const now = Date.now();
                const lastReload = window.lastTemplateReload || 0;
                
                if (now - lastReload > 500) {
                    window.lastTemplateReload = now;
                    
                    // Add a brief delay to ensure that all templates are saved
                    setTimeout(() => {
                        // Force reload the page
                        window.location.reload();
                    }, 100);
                } else {
                    console.log('[Vigil] Skipping duplicate reload - too soon after previous reload');
                }
            } else if (message.startsWith('connected:')) {
                // Extract connection ID for improved logging
                connectionId = message.substring(10);
                console.log(`[Vigil] Connection confirmed by server [id=${connectionId}]`);
            } else if (message.startsWith('ping:')) {
                // Update last ping time for connection health monitoring
                lastPingTime = Date.now();
                // We don't need to respond to pings as the server is using them just to keep the connection alive
            }
        });
        
        // Add connection health check timer
        const healthCheckInterval = setInterval(() => {
            const now = Date.now();
            // If we haven't received a ping in 30 seconds, consider the connection dead
            if (now - lastPingTime > 30000) {
                console.warn(`[Vigil] No ping received in over 30 seconds, connection may be dead [id=${connectionId}]`);
                clearInterval(healthCheckInterval);
                ws.close();
            }
        }, 5000); // Check every 5 seconds
        
        // Connection closed
        ws.addEventListener('close', (event) => {
            // Clear health check interval
            clearInterval(healthCheckInterval);
            
            if (!isReconnecting) {
                console.log(`[Vigil] Disconnected from template reload websocket [id=${connectionId}], attempting to reconnect...`);
                attemptReconnect();
            }
        });
        
        // Connection error
        ws.addEventListener('error', (event) => {
            console.error('[Vigil] WebSocket error:', event);
            // Don't attempt to reconnect here, let the close handler do it
        });
        
        const pingInterval = setInterval(() => {
            if (ws.readyState === WebSocket.OPEN) {
                ws.send('ping');
            } else if (ws.readyState === WebSocket.CLOSED || ws.readyState === WebSocket.CLOSING) {
                clearInterval(pingInterval);
            }
        }, 5000);
        
        return ws;
    }
    
    function attemptReconnect() {
        isReconnecting = true;
        reconnectAttempts++;
        
        // Exponential backoff with a maximum of 5 seconds
        const delay = Math.min(500 * Math.pow(1.5, reconnectAttempts - 1), 5000);
        
        console.log(`[Vigil] Attempting to reconnect in ${delay}ms (attempt ${reconnectAttempts})...`);
        
        // Schedule reconnection
        reconnectTimer = setTimeout(() => {
            console.log(`[Vigil] Reconnecting now (attempt ${reconnectAttempts})...`);
            connectWebSocket();
        }, delay);
    }
    
    // Initial connection
    connectWebSocket();
    
    // Notify that Vigil is active
    console.log('%c[Vigil] Template hot reload enabled', 'color: #8c16a1; font-weight: bold;');
})();
