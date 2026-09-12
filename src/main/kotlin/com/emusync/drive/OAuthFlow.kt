package com.emusync.drive

import com.emusync.config.ConfigManager
import com.emusync.model.AppConfig
import com.emusync.model.GoogleDriveConfig
import com.sun.net.httpserver.HttpServer
import io.ktor.client.HttpClient
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlinx.coroutines.withContext
import java.awt.Desktop
import java.net.InetSocketAddress
import java.net.URI
import java.net.URLDecoder
import java.net.URLEncoder
import kotlin.coroutines.resume
import kotlin.coroutines.resumeWithException

/**
 * Handles the full OAuth 2.0 authorization flow for desktop apps using the
 * loopback redirect method (Google's recommended approach for installed apps).
 *
 * Flow:
 * 1. Starts a temporary HTTP server on `http://localhost:PORT`.
 * 2. Opens the user's default browser to Google's consent screen.
 * 3. User authorizes → Google redirects to localhost with an auth code.
 * 4. The local server captures the code and shuts down.
 * 5. Exchanges the code for access + refresh tokens via [DriveAuth].
 * 6. Persists the refresh token to `config.json` via [ConfigManager].
 *
 * Usage:
 * ```
 * val flow = OAuthFlow(httpClient)
 * val accessToken = flow.authorize(config, configManager)
 * ```
 */
class OAuthFlow(private val client: HttpClient) {

    companion object {
        private const val AUTH_URL = "https://accounts.google.com/o/oauth2/v2/auth"
        private const val LOOPBACK_PORT = 8089
        private const val REDIRECT_URI = "http://localhost:$LOOPBACK_PORT"
        private const val SCOPE = "https://www.googleapis.com/auth/drive.file"

        /** HTML shown to the user after successful authorization. */
        private val SUCCESS_HTML = """
            <!DOCTYPE html>
            <html lang="en">
            <head>
                <meta charset="UTF-8">
                <meta name="viewport" content="width=device-width, initial-scale=1.0">
                <title>EmuSync — Authorized</title>
                <link rel="preconnect" href="https://fonts.googleapis.com">
                <link href="https://fonts.googleapis.com/css2?family=Inter:wght@400;500;600;700&display=swap" rel="stylesheet">
                <style>
                    *, *::before, *::after { margin: 0; padding: 0; box-sizing: border-box; }
                    body {
                        font-family: 'Inter', system-ui, -apple-system, sans-serif;
                        min-height: 100vh;
                        display: flex;
                        justify-content: center;
                        align-items: center;
                        background: linear-gradient(135deg, #0f0c29 0%, #1a1a3e 40%, #24243e 100%);
                        overflow: hidden;
                    }
                    /* Animated background orbs */
                    .orb {
                        position: fixed;
                        border-radius: 50%;
                        filter: blur(80px);
                        opacity: 0.3;
                        animation: float 8s ease-in-out infinite;
                    }
                    .orb-1 { width: 400px; height: 400px; background: #6c63ff; top: -100px; left: -100px; }
                    .orb-2 { width: 300px; height: 300px; background: #00d2ff; bottom: -80px; right: -80px; animation-delay: -4s; }
                    .orb-3 { width: 200px; height: 200px; background: #a78bfa; top: 50%; left: 60%; animation-delay: -2s; }
                    @keyframes float {
                        0%, 100% { transform: translateY(0) scale(1); }
                        50% { transform: translateY(-30px) scale(1.05); }
                    }
                    .card {
                        position: relative;
                        z-index: 1;
                        text-align: center;
                        padding: 3rem 3.5rem;
                        background: rgba(255, 255, 255, 0.06);
                        backdrop-filter: blur(24px);
                        -webkit-backdrop-filter: blur(24px);
                        border-radius: 24px;
                        border: 1px solid rgba(255, 255, 255, 0.1);
                        box-shadow: 0 8px 32px rgba(0, 0, 0, 0.4), inset 0 1px 0 rgba(255,255,255,0.08);
                        animation: slideUp 0.6s cubic-bezier(0.16, 1, 0.3, 1) forwards;
                        opacity: 0;
                        max-width: 440px;
                        width: 90vw;
                    }
                    @keyframes slideUp {
                        from { opacity: 0; transform: translateY(40px) scale(0.96); }
                        to { opacity: 1; transform: translateY(0) scale(1); }
                    }
                    .icon-wrap {
                        width: 80px; height: 80px;
                        margin: 0 auto 1.5rem;
                        border-radius: 50%;
                        background: linear-gradient(135deg, #22c55e, #16a34a);
                        display: flex;
                        align-items: center;
                        justify-content: center;
                        box-shadow: 0 0 40px rgba(34, 197, 94, 0.3);
                        animation: pop 0.5s 0.3s cubic-bezier(0.34, 1.56, 0.64, 1) both;
                    }
                    @keyframes pop {
                        from { transform: scale(0); }
                        to { transform: scale(1); }
                    }
                    .checkmark {
                        width: 40px; height: 40px;
                        stroke: white; stroke-width: 3;
                        fill: none; stroke-linecap: round; stroke-linejoin: round;
                        stroke-dasharray: 60; stroke-dashoffset: 60;
                        animation: draw 0.5s 0.6s ease forwards;
                    }
                    @keyframes draw {
                        to { stroke-dashoffset: 0; }
                    }
                    h1 {
                        font-size: 1.5rem;
                        font-weight: 700;
                        color: #f1f5f9;
                        margin-bottom: 0.5rem;
                        letter-spacing: -0.01em;
                    }
                    .subtitle {
                        font-size: 0.95rem;
                        color: #94a3b8;
                        line-height: 1.5;
                        margin-bottom: 1.5rem;
                    }
                    .brand {
                        display: inline-flex;
                        align-items: center;
                        gap: 6px;
                        font-size: 0.8rem;
                        font-weight: 600;
                        color: rgba(167, 139, 250, 0.8);
                        letter-spacing: 0.05em;
                        text-transform: uppercase;
                    }
                    .countdown {
                        font-size: 0.8rem;
                        color: #64748b;
                        margin-top: 1rem;
                    }
                </style>
            </head>
            <body>
                <div class="orb orb-1"></div>
                <div class="orb orb-2"></div>
                <div class="orb orb-3"></div>
                <div class="card">
                    <div class="icon-wrap">
                        <svg class="checkmark" viewBox="0 0 40 40">
                            <polyline points="10,20 18,28 30,12"/>
                        </svg>
                    </div>
                    <h1>Authorization Successful</h1>
                    <p class="subtitle">Your Google Drive is connected.<br>You can close this tab and return to EmuSync.</p>
                    <div class="brand">🎮 EmuSync</div>
                    <p class="countdown" id="cd">This tab will close in <strong>5</strong>s</p>
                </div>
                <script>
                    let s = 5;
                    const el = document.querySelector('#cd strong');
                    const t = setInterval(() => {
                        s--;
                        el.textContent = s;
                        if (s <= 0) { clearInterval(t); window.close(); }
                    }, 1000);
                </script>
            </body>
            </html>
        """.trimIndent()

        /** HTML shown on authorization error. */
        private val ERROR_HTML = """
            <!DOCTYPE html>
            <html lang="en">
            <head>
                <meta charset="UTF-8">
                <meta name="viewport" content="width=device-width, initial-scale=1.0">
                <title>EmuSync — Error</title>
                <link rel="preconnect" href="https://fonts.googleapis.com">
                <link href="https://fonts.googleapis.com/css2?family=Inter:wght@400;500;600;700&display=swap" rel="stylesheet">
                <style>
                    *, *::before, *::after { margin: 0; padding: 0; box-sizing: border-box; }
                    body {
                        font-family: 'Inter', system-ui, -apple-system, sans-serif;
                        min-height: 100vh;
                        display: flex;
                        justify-content: center;
                        align-items: center;
                        background: linear-gradient(135deg, #1a0a0a 0%, #2d1515 40%, #1a0f0f 100%);
                        overflow: hidden;
                    }
                    .orb {
                        position: fixed;
                        border-radius: 50%;
                        filter: blur(80px);
                        opacity: 0.25;
                        animation: float 8s ease-in-out infinite;
                    }
                    .orb-1 { width: 400px; height: 400px; background: #ef4444; top: -100px; left: -100px; }
                    .orb-2 { width: 300px; height: 300px; background: #f97316; bottom: -80px; right: -80px; animation-delay: -4s; }
                    @keyframes float {
                        0%, 100% { transform: translateY(0) scale(1); }
                        50% { transform: translateY(-30px) scale(1.05); }
                    }
                    .card {
                        position: relative;
                        z-index: 1;
                        text-align: center;
                        padding: 3rem 3.5rem;
                        background: rgba(255, 255, 255, 0.05);
                        backdrop-filter: blur(24px);
                        -webkit-backdrop-filter: blur(24px);
                        border-radius: 24px;
                        border: 1px solid rgba(239, 68, 68, 0.2);
                        box-shadow: 0 8px 32px rgba(0, 0, 0, 0.5);
                        animation: slideUp 0.6s cubic-bezier(0.16, 1, 0.3, 1) forwards;
                        opacity: 0;
                        max-width: 440px;
                        width: 90vw;
                    }
                    @keyframes slideUp {
                        from { opacity: 0; transform: translateY(40px) scale(0.96); }
                        to { opacity: 1; transform: translateY(0) scale(1); }
                    }
                    .icon-wrap {
                        width: 80px; height: 80px;
                        margin: 0 auto 1.5rem;
                        border-radius: 50%;
                        background: linear-gradient(135deg, #ef4444, #dc2626);
                        display: flex;
                        align-items: center;
                        justify-content: center;
                        box-shadow: 0 0 40px rgba(239, 68, 68, 0.3);
                        animation: pop 0.5s 0.3s cubic-bezier(0.34, 1.56, 0.64, 1) both;
                    }
                    @keyframes pop {
                        from { transform: scale(0); }
                        to { transform: scale(1); }
                    }
                    .x-mark {
                        width: 36px; height: 36px;
                        stroke: white; stroke-width: 3;
                        fill: none; stroke-linecap: round;
                        stroke-dasharray: 50; stroke-dashoffset: 50;
                        animation: draw 0.4s 0.6s ease forwards;
                    }
                    @keyframes draw {
                        to { stroke-dashoffset: 0; }
                    }
                    h1 {
                        font-size: 1.5rem;
                        font-weight: 700;
                        color: #fca5a5;
                        margin-bottom: 0.5rem;
                    }
                    .subtitle {
                        font-size: 0.95rem;
                        color: #94a3b8;
                        line-height: 1.5;
                        margin-bottom: 1.5rem;
                    }
                    .brand {
                        font-size: 0.8rem;
                        font-weight: 600;
                        color: rgba(252, 165, 165, 0.6);
                        letter-spacing: 0.05em;
                        text-transform: uppercase;
                    }
                </style>
            </head>
            <body>
                <div class="orb orb-1"></div>
                <div class="orb orb-2"></div>
                <div class="card">
                    <div class="icon-wrap">
                        <svg class="x-mark" viewBox="0 0 36 36">
                            <line x1="10" y1="10" x2="26" y2="26"/>
                            <line x1="26" y1="10" x2="10" y2="26"/>
                        </svg>
                    </div>
                    <h1>Authorization Failed</h1>
                    <p class="subtitle">Something went wrong.<br>Please close this tab and try again from EmuSync.</p>
                    <div class="brand">🎮 EmuSync</div>
                </div>
            </body>
            </html>
        """.trimIndent()
    }

    /**
     * Runs the authorization flow.
     *
     * If [allowInteractive] is false, this will ONLY attempt background refresh with stored credentials.
     * If the user is offline or no refresh token is present, it throws [DriveApiException] immediately
     * without blocking, showing dialogs, or opening browser windows.
     *
     * @param config            Current [AppConfig].
     * @param configManager     Used to persist the refresh token after first auth.
     * @param allowInteractive  If true, triggers browser OAuth when no valid refresh token is available.
     *                          If false, throws [DriveApiException] immediately if offline/unauthenticated.
     * @return A valid access token ready for Drive API calls.
     * @throws DriveApiException if authorization fails or offline.
     * @throws IllegalStateException if `googleDrive` config section is missing.
     */
    suspend fun authorize(
        config: AppConfig,
        configManager: ConfigManager,
        allowInteractive: Boolean = false,
    ): String {
        val driveConfig = requireNotNull(config.googleDrive) {
            "Missing 'googleDrive' section in config.json. " +
            "Please add clientId and clientSecret from Google Cloud Console."
        }

        val auth = DriveAuth(client)

        // Fast path: we already have a refresh token → try to refresh
        if (!driveConfig.refreshToken.isNullOrBlank()) {
            try {
                val tokenResponse = auth.refreshAccessToken(
                    clientId = driveConfig.clientId,
                    clientSecret = driveConfig.clientSecret,
                    refreshToken = driveConfig.refreshToken,
                )
                return tokenResponse.accessToken
            } catch (e: Exception) {
                val message = e.message ?: ""
                if ("invalid_grant" in message) {
                    // Refresh token expired or revoked (common in Google "Testing" mode).
                    // Clear the bad token.
                    val clearedConfig = config.copy(
                        googleDrive = driveConfig.copy(refreshToken = null),
                    )
                    configManager.save(clearedConfig)
                } else {
                    // Network failure, DNS resolution failure, socket timeout (offline mode)
                    throw DriveApiException("Drive offline or unreachable: $message", e)
                }
            }
        }

        // If interactive browser flow is NOT permitted (e.g. during game launch):
        if (!allowInteractive) {
            throw DriveApiException("Google Drive is not authenticated or offline. Proceeding with local saves.")
        }

        // Slow path: first-time authorization via loopback with timeout
        val code = startLoopbackAndGetCode(driveConfig.clientId)

        val tokens = auth.exchangeCodeForTokens(
            clientId = driveConfig.clientId,
            clientSecret = driveConfig.clientSecret,
            code = code,
            redirectUri = REDIRECT_URI,
        )

        // Persist the refresh token so we don't need to re-authorize next time
        val updatedConfig = config.copy(
            googleDrive = driveConfig.copy(refreshToken = tokens.refreshToken),
        )
        configManager.save(updatedConfig)

        return tokens.accessToken
    }

    /**
     * Starts a local HTTP server, opens the browser to Google's consent page,
     * and suspends until the authorization code is received (with 60s timeout).
     */
    private suspend fun startLoopbackAndGetCode(clientId: String): String = kotlinx.coroutines.withTimeout(60_000) {
        // Build the authorization URL
        val authUrl = buildAuthUrl(clientId)

        return@withTimeout suspendCancellableCoroutine { continuation ->
            val server = HttpServer.create(InetSocketAddress(LOOPBACK_PORT), 0)

            continuation.invokeOnCancellation {
                server.stop(0)
            }

            server.createContext("/") { exchange ->
                val query = exchange.requestURI.query ?: ""
                val params = parseQueryParams(query)

                val code = params["code"]
                val error = params["error"]

                val (statusCode, html) = when {
                    code != null -> 200 to SUCCESS_HTML
                    else -> 400 to ERROR_HTML
                }

                val responseBytes = html.toByteArray()
                exchange.responseHeaders.add("Content-Type", "text/html; charset=utf-8")
                exchange.sendResponseHeaders(statusCode, responseBytes.size.toLong())
                exchange.responseBody.use { it.write(responseBytes) }

                // Stop the server after responding
                server.stop(1)

                when {
                    code != null -> continuation.resume(code)
                    error != null -> continuation.resumeWithException(
                        DriveApiException("User denied authorization: $error")
                    )
                    else -> continuation.resumeWithException(
                        DriveApiException("No authorization code received")
                    )
                }
            }

            server.start()

            // Open the browser
            try {
                val os = System.getProperty("os.name", "").lowercase()
                val isWindows = os.contains("windows")

                if (isWindows) {
                    try {
                        if (Desktop.isDesktopSupported() && Desktop.getDesktop().isSupported(Desktop.Action.BROWSE)) {
                            Desktop.getDesktop().browse(URI(authUrl))
                        } else {
                            ProcessBuilder("rundll32", "url.dll,FileProtocolHandler", authUrl).start()
                        }
                    } catch (_: Throwable) {
                        try {
                            ProcessBuilder("rundll32", "url.dll,FileProtocolHandler", authUrl).start()
                        } catch (_: Throwable) {
                            ProcessBuilder("cmd", "/c", "start", "\"\"", authUrl.replace("&", "^&")).start()
                        }
                    }
                } else {
                    if (Desktop.isDesktopSupported() && Desktop.getDesktop().isSupported(Desktop.Action.BROWSE)) {
                        Desktop.getDesktop().browse(URI(authUrl))
                    } else {
                        // Fallback for headless environments (SteamOS Game Mode)
                        Runtime.getRuntime().exec(arrayOf("xdg-open", authUrl))
                    }
                }
            } catch (e: Exception) {
                server.stop(0)
                continuation.resumeWithException(
                    DriveApiException("Could not open browser. Please visit manually:\n$authUrl", e)
                )
            }
        }
    }

    /**
     * Builds the Google OAuth 2.0 authorization URL.
     */
    private fun buildAuthUrl(clientId: String): String {
        val params = mapOf(
            "client_id" to clientId,
            "redirect_uri" to REDIRECT_URI,
            "response_type" to "code",
            "scope" to SCOPE,
            "access_type" to "offline",     // Ensures we get a refresh_token
            "prompt" to "consent",          // Forces consent screen (always returns refresh_token)
        )

        val queryString = params.entries.joinToString("&") { (key, value) ->
            "${URLEncoder.encode(key, "UTF-8")}=${URLEncoder.encode(value, "UTF-8")}"
        }

        return "$AUTH_URL?$queryString"
    }

    /**
     * Parses a URI query string into a map.
     */
    private fun parseQueryParams(query: String): Map<String, String> {
        if (query.isBlank()) return emptyMap()
        return query.split("&").associate { param ->
            val (key, value) = param.split("=", limit = 2)
            key to URLDecoder.decode(value, "UTF-8")
        }
    }
}
