// Vigil template hot reload client
(function() {
    console.log('[Vigil] Loading hot reload client');
    
    // Handle errors from missing scripts quietly
    window.addEventListener('error', function(event) {
        if (event.target?.tagName === 'SCRIPT') {
            const src = event.target.src || '';
            if (src.includes('/app.min.js') || src.includes('/app/app.min.js')) {
                console.warn('[Vigil] Ignoring error for missing script:', src);
                event.preventDefault();
                return true;
            }
        }
    }, true);
    
    // Track reload and connection state
    window.lastTemplateReload = 0;
    let reconnectTimer = null;
    let reconnectAttempts = 0;
    let isReconnecting = false;
    
    function connectWebSocket() {
        if (reconnectTimer) {
            clearTimeout(reconnectTimer);
            reconnectTimer = null;
        }
        
        // Create WebSocket connection
        const ws = new WebSocket(`ws://${window.location.host}/ws/dev/reload`);
        
        // Connection tracking
        let lastResponseTime = Date.now();
        let connectionId = null;
        let lastChangeTimestamp = 0;
        
        // Set up ping interval (every second)
        const pingInterval = setInterval(() => {
            if (ws.readyState === WebSocket.OPEN) {
                ws.send('ping');
            }
        }, 1000);
        
        // Message handler
        ws.addEventListener('message', (event) => {
            const message = event.data;
            lastResponseTime = Date.now();
            
            if (message.startsWith('time:')) {
                // Process timestamp message
                const serverTimestamp = parseInt(message.substring(5), 10);
                
                if (serverTimestamp > lastChangeTimestamp) {
                    if (lastChangeTimestamp > 0) {
                        console.log(`[Vigil] File changes detected, reloading...`);
                        window.location.reload();
                    } else {
                        console.debug(`[Vigil] Initial timestamp: ${serverTimestamp}`);
                    }
                    lastChangeTimestamp = serverTimestamp;
                }
            } else if (message.startsWith('reload:')) {
                // Process direct reload message
                const filePath = message.substring(7);
                console.log(`[Vigil] File changed: ${filePath}, reloading...`);
                window.location.reload();
            } else if (message.startsWith('connected:')) {
                // Process connection ID
                connectionId = message.substring(10);
                console.log(`[Vigil] Connected [id=${connectionId}]`);
            }
        });
        
        // Health check (every 5 seconds)
        const healthCheckInterval = setInterval(() => {
            if (Date.now() - lastResponseTime > 10000) {
                console.warn(`[Vigil] Connection timeout, reconnecting...`);
                clearInterval(healthCheckInterval);
                clearInterval(pingInterval);
                try { ws.close(1001, "No response"); } catch (e) {}
                attemptReconnect();
            }
        }, 5000);
        
        // Handle connection close
        ws.addEventListener('close', () => {
            clearInterval(healthCheckInterval);
            clearInterval(pingInterval);
            if (!isReconnecting) {
                attemptReconnect();
            }
        });
        
        // Silent error handling
        ws.addEventListener('error', () => {});
        
        // On open handler
        ws.addEventListener('open', () => {
            reconnectAttempts = 0;
            isReconnecting = false;
            console.log('[Vigil] Connected to hot reload service');
        });
        
        return ws;
    }
    
    function attemptReconnect() {
        isReconnecting = true;
        reconnectAttempts++;
        // Exponential backoff with 5 second maximum
        const delay = Math.min(500 * Math.pow(1.5, reconnectAttempts - 1), 5000);
        reconnectTimer = setTimeout(connectWebSocket, delay);
    }
    
    // Initial connection
    connectWebSocket();
    
    console.log('%c[Vigil] Hot reload enabled', 'color: #8c16a1; font-weight: bold;');
})();
