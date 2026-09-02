package com.emusync.ui.theme

import androidx.compose.material3.darkColorScheme
import androidx.compose.ui.graphics.Color

/**
 * EmuSync dark theme colors — a premium dark palette inspired by gaming UIs.
 */
object EmuSyncColors {
    // Main background tones
    val Background = Color(0xFF0D1117)
    val Surface = Color(0xFF161B22)
    val SurfaceVariant = Color(0xFF1C2333)
    val SurfaceSelected = Color(0xFF1F2A40)

    // Accent — vibrant blue-purple
    val Primary = Color(0xFF58A6FF)
    val PrimaryContainer = Color(0xFF1A3A5C)
    val Secondary = Color(0xFF7C3AED)
    val SecondaryContainer = Color(0xFF2D1B69)

    // State colors
    val Success = Color(0xFF3FB950)
    val Warning = Color(0xFFF0883E)
    val Error = Color(0xFFF85149)

    // Text
    val OnBackground = Color(0xFFE6EDF3)
    val OnSurface = Color(0xFFC9D1D9)
    val OnSurfaceDim = Color(0xFF8B949E)

    // Divider
    val Divider = Color(0xFF30363D)

    // Card hover
    val CardHover = Color(0xFF222D3D)
}

val EmuSyncDarkScheme = darkColorScheme(
    primary = EmuSyncColors.Primary,
    onPrimary = Color.White,
    primaryContainer = EmuSyncColors.PrimaryContainer,
    secondary = EmuSyncColors.Secondary,
    onSecondary = Color.White,
    secondaryContainer = EmuSyncColors.SecondaryContainer,
    background = EmuSyncColors.Background,
    onBackground = EmuSyncColors.OnBackground,
    surface = EmuSyncColors.Surface,
    onSurface = EmuSyncColors.OnSurface,
    surfaceVariant = EmuSyncColors.SurfaceVariant,
    onSurfaceVariant = EmuSyncColors.OnSurfaceDim,
    outline = EmuSyncColors.Divider,
    error = EmuSyncColors.Error,
    onError = Color.White,
)
