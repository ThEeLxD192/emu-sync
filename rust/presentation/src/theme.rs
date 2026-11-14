//! EmuSync theme, typography and color palette.

use egui::{
    Color32, CornerRadius, FontData, FontFamily, FontId, Stroke, TextStyle, Visuals,
    epaint::text::{
        FontInsert, FontPriority, FontTweak, HintingTarget, InsertFontFamily, SmoothHinting,
    },
};

pub const BACKGROUND: Color32 = Color32::from_rgb(0x0D, 0x11, 0x17);
pub const SURFACE: Color32 = Color32::from_rgb(0x16, 0x1B, 0x22);
pub const SURFACE_VARIANT: Color32 = Color32::from_rgb(0x1C, 0x23, 0x33);
pub const SURFACE_SELECTED: Color32 = Color32::from_rgb(0x1F, 0x2A, 0x40);

pub const PRIMARY: Color32 = Color32::from_rgb(0x58, 0xA6, 0xFF);
pub const PRIMARY_CONTAINER: Color32 = Color32::from_rgb(0x1A, 0x3A, 0x5C);
pub const SECONDARY: Color32 = Color32::from_rgb(0x7C, 0x3A, 0xED);
pub const SECONDARY_CONTAINER: Color32 = Color32::from_rgb(0x2D, 0x1B, 0x69);

pub const SUCCESS: Color32 = Color32::from_rgb(0x3F, 0xB9, 0x50);
pub const WARNING: Color32 = Color32::from_rgb(0xF0, 0x88, 0x3E);
pub const ERROR: Color32 = Color32::from_rgb(0xF8, 0x51, 0x49);

pub const ON_BACKGROUND: Color32 = Color32::from_rgb(0xE6, 0xED, 0xF3);
pub const ON_SURFACE: Color32 = Color32::from_rgb(0xC9, 0xD1, 0xD9);
pub const ON_SURFACE_DIM: Color32 = Color32::from_rgb(0x8B, 0x94, 0x9E);

pub const DIVIDER: Color32 = Color32::from_rgb(0x30, 0x36, 0x3D);
pub const CARD_HOVER: Color32 = Color32::from_rgb(0x22, 0x2D, 0x3D);

/// JetBrainsMono Nerd Font (SIL OFL 1.1, see `assets/fonts/OFL.txt`).
const FONT_REGULAR_NAME: &str = "JetBrainsMonoNerdFont-Regular";
const FONT_BOLD_NAME: &str = "JetBrainsMonoNerdFont-Bold";
const FONT_REGULAR: &[u8] = include_bytes!("../assets/fonts/JetBrainsMonoNerdFont-Regular.ttf");
const FONT_BOLD: &[u8] = include_bytes!("../assets/fonts/JetBrainsMonoNerdFont-Bold.ttf");
const BOLD_FAMILY: &str = "bold";

/// Nerd Font glyphs (Material Design range) used as icons.
pub mod glyph {
    pub const CLOUD: &str = "\u{F015F}";
    pub const CLOUD_SYNC: &str = "\u{F063F}";
    pub const UPDATE: &str = "\u{F06B0}";
    pub const RESTART: &str = "\u{F0709}";
    pub const DOWNLOAD: &str = "\u{F01DA}";
    pub const PLUS: &str = "\u{F0415}";
    pub const FOLDER_OPEN: &str = "\u{F0770}";
    pub const CONTROLLER: &str = "\u{F0B82}";
    pub const GAMEPAD: &str = "\u{F0296}";
    pub const DRAG: &str = "\u{F01DD}";
    pub const CHEVRON_UP: &str = "\u{F0143}";
    pub const CHEVRON_DOWN: &str = "\u{F0140}";
    pub const CHEVRON_RIGHT: &str = "\u{F0142}";
    pub const WIDGETS: &str = "\u{F072C}";
    pub const PLAY: &str = "\u{F040A}";
    pub const PENCIL: &str = "\u{F03EB}";
    pub const DELETE: &str = "\u{F01B4}";
    pub const COG: &str = "\u{F0493}";
    pub const TRANSLATE: &str = "\u{F05CA}";
    pub const CLOUD_CHECK: &str = "\u{F0160}";
    pub const CLOUD_DOWNLOAD: &str = "\u{F0162}";
    pub const ALERT: &str = "\u{F0026}";
    pub const REFRESH: &str = "\u{F0450}";
    pub const IMAGE: &str = "\u{F02E9}";
    pub const HOURGLASS: &str = "\u{F051F}";
    pub const COMPUTER: &str = "\u{F0379}";
    pub const ERROR_OUTLINE: &str = "\u{F05D6}";
    pub const INFO: &str = "\u{F02FD}";
    pub const LINK_OFF: &str = "\u{F0338}";
    pub const EYE: &str = "\u{F0208}";
    pub const EYE_OFF: &str = "\u{F0209}";
}

/// Font family rendering the bold weight of the application font.
pub fn bold_family() -> FontFamily {
    FontFamily::Name(BOLD_FAMILY.into())
}

/// Font id for bold text at the given size.
pub fn bold(size: f32) -> FontId {
    FontId::new(size, bold_family())
}

/// Sharpens the bundled fonts at small sizes without changing text layout.
///
/// JetBrains Mono ships its own TrueType hinting. egui's defaults render each
/// glyph at fractional offsets and position-independent, which widens stems
/// and blurs that hinting; whole-pixel placement and full grid-fitting keep
/// the stems crisp. Glyph positions still come from the shaper's advances.
fn crisp_font(bytes: &'static [u8]) -> FontData {
    FontData::from_static(bytes).tweak(FontTweak {
        hinting_target: HintingTarget::Smooth(SmoothHinting {
            light: false,
            symmetric_rendering: false,
            preserve_linear_metrics: false,
        }),
        subpixel_binning: Some(false),
        ..FontTweak::default()
    })
}

/// Registers the application fonts once; later calls only compare font names.
fn install_fonts(ctx: &egui::Context) {
    let primary = |family| InsertFontFamily {
        family,
        priority: FontPriority::Highest,
    };
    ctx.add_font(FontInsert::new(
        FONT_REGULAR_NAME,
        crisp_font(FONT_REGULAR),
        vec![
            primary(FontFamily::Proportional),
            primary(FontFamily::Monospace),
            InsertFontFamily {
                family: bold_family(),
                priority: FontPriority::Lowest,
            },
        ],
    ));
    ctx.add_font(FontInsert::new(
        FONT_BOLD_NAME,
        crisp_font(FONT_BOLD),
        vec![primary(bold_family())],
    ));
}

/// Configures the egui context with the EmuSync fonts, dark palette, roundings, and widget styles.
pub fn configure_egui_theme(ctx: &egui::Context) {
    install_fonts(ctx);

    ctx.all_styles_mut(|style| {
        style.text_styles = [
            (TextStyle::Small, FontId::proportional(12.0)),
            (TextStyle::Body, FontId::proportional(13.0)),
            (TextStyle::Button, FontId::proportional(13.0)),
            (TextStyle::Monospace, FontId::monospace(13.0)),
            (TextStyle::Heading, bold(20.0)),
        ]
        .into();
        style.spacing.button_padding = egui::vec2(12.0, 6.0);
    });

    let mut visuals = Visuals::dark();

    visuals.panel_fill = BACKGROUND;
    visuals.window_fill = SURFACE;
    visuals.window_stroke = Stroke::new(1.0, DIVIDER);
    visuals.window_corner_radius = CornerRadius::same(12);

    visuals.widgets.noninteractive.bg_fill = SURFACE;
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, ON_SURFACE);
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, DIVIDER);
    visuals.widgets.noninteractive.corner_radius = CornerRadius::same(6);

    visuals.widgets.inactive.bg_fill = SURFACE_VARIANT;
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, ON_SURFACE);
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, DIVIDER);
    visuals.widgets.inactive.weak_bg_fill = SURFACE_VARIANT;
    visuals.widgets.inactive.corner_radius = CornerRadius::same(8);

    visuals.widgets.hovered.bg_fill = CARD_HOVER;
    visuals.widgets.hovered.weak_bg_fill = CARD_HOVER;
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, ON_BACKGROUND);
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, DIVIDER);
    visuals.widgets.hovered.corner_radius = CornerRadius::same(8);
    visuals.widgets.hovered.expansion = 0.0;

    visuals.widgets.active.bg_fill = PRIMARY_CONTAINER;
    visuals.widgets.active.weak_bg_fill = PRIMARY_CONTAINER;
    visuals.widgets.active.fg_stroke = Stroke::new(1.0, PRIMARY);
    visuals.widgets.active.bg_stroke = Stroke::new(1.0, PRIMARY);
    visuals.widgets.active.corner_radius = CornerRadius::same(8);
    visuals.widgets.active.expansion = 0.0;

    visuals.selection.bg_fill = PRIMARY_CONTAINER;
    visuals.selection.stroke = Stroke::new(1.0, PRIMARY);

    ctx.set_visuals(visuals);
}
