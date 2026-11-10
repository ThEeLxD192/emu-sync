use crate::{
    steam::shortcut::{SteamShortcut, VdfValue},
    util::atomic::write_atomic,
};
use std::{
    collections::BTreeMap,
    fs,
    io::{self, Cursor, Read},
    path::Path,
};

const TYPE_OBJECT: u8 = 0x00;
const TYPE_STRING: u8 = 0x01;
const TYPE_INT32: u8 = 0x02;
const TYPE_END: u8 = 0x08;
const MAX_OBJECT_DEPTH: usize = 64;

pub struct BinaryVdf;

impl BinaryVdf {
    pub fn read(path: &Path) -> io::Result<Vec<SteamShortcut>> {
        if !path.exists() {
            return Ok(Vec::new());
        }
        let bytes = fs::read(path)?;
        if bytes.is_empty() {
            return Ok(Vec::new());
        }
        Self::read_bytes(&bytes)
    }

    pub fn read_bytes(bytes: &[u8]) -> io::Result<Vec<SteamShortcut>> {
        if bytes.is_empty() {
            return Ok(Vec::new());
        }
        let mut cursor = Cursor::new(bytes);

        let mut root_type = [0u8; 1];
        cursor.read_exact(&mut root_type)?;
        if root_type[0] != TYPE_OBJECT {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Expected object marker (0x00), got 0x{:02x}", root_type[0]),
            ));
        }

        let root_name = read_null_term_string(&mut cursor)?;
        if root_name != "shortcuts" {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Expected 'shortcuts' root object, got '{root_name}'"),
            ));
        }

        let mut shortcuts = Vec::new();

        loop {
            let mut marker = [0u8; 1];
            cursor.read_exact(&mut marker)?;
            if marker[0] == TYPE_END {
                break;
            }
            if marker[0] != TYPE_OBJECT {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "Expected object (0x00) or end (0x08), got 0x{:02x}",
                        marker[0]
                    ),
                ));
            }

            // Index name ("0", "1", ...)
            let _ = read_null_term_string(&mut cursor)?;
            let fields = read_object_fields(&mut cursor, 0)?;
            shortcuts.push(build_shortcut(fields));
        }

        Ok(shortcuts)
    }

    pub fn write(path: &Path, shortcuts: &[SteamShortcut]) -> io::Result<()> {
        Self::write_with_hook(path, shortcuts, |_| Ok(()))
    }

    pub fn write_with_hook<F>(
        path: &Path,
        shortcuts: &[SteamShortcut],
        before_replace: F,
    ) -> io::Result<()>
    where
        F: FnOnce(&Path) -> io::Result<()>,
    {
        let mut buf = Vec::new();

        // Root object "shortcuts"
        buf.push(TYPE_OBJECT);
        write_null_term_string(&mut buf, "shortcuts")?;

        for (index, shortcut) in shortcuts.iter().enumerate() {
            buf.push(TYPE_OBJECT);
            write_null_term_string(&mut buf, &index.to_string())?;
            write_shortcut_fields(&mut buf, shortcut)?;
            buf.push(TYPE_END);
        }

        buf.push(TYPE_END);
        buf.push(TYPE_END);

        write_atomic(path, &buf, None, before_replace)
    }
}

fn read_null_term_string(cursor: &mut Cursor<&[u8]>) -> io::Result<String> {
    let data: &[u8] = cursor.get_ref();
    let start = usize::try_from(cursor.position())
        .unwrap_or(usize::MAX)
        .min(data.len());
    let rest = &data[start..];
    let len = rest
        .iter()
        .position(|&b| b == 0)
        .ok_or_else(|| io::Error::new(io::ErrorKind::UnexpectedEof, "Unterminated VDF string"))?;
    let text = std::str::from_utf8(&rest[..len])
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("Invalid UTF-8: {e}")))?
        .to_string();
    cursor.set_position((start + len + 1) as u64);
    Ok(text)
}

fn read_int32_le(cursor: &mut Cursor<&[u8]>) -> io::Result<i32> {
    let mut buf = [0u8; 4];
    cursor.read_exact(&mut buf)?;
    Ok(i32::from_le_bytes(buf))
}

fn write_null_term_string(buf: &mut Vec<u8>, val: &str) -> io::Result<()> {
    if val.contains('\0') {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "VDF strings cannot contain NUL",
        ));
    }
    buf.extend_from_slice(val.as_bytes());
    buf.push(0);
    Ok(())
}

fn write_string_field(buf: &mut Vec<u8>, key: &str, val: &str) -> io::Result<()> {
    buf.push(TYPE_STRING);
    write_null_term_string(buf, key)?;
    write_null_term_string(buf, val)?;
    Ok(())
}

fn write_int32_field(buf: &mut Vec<u8>, key: &str, val: i32) -> io::Result<()> {
    buf.push(TYPE_INT32);
    write_null_term_string(buf, key)?;
    buf.extend_from_slice(&val.to_le_bytes());
    Ok(())
}

fn read_object_fields(
    cursor: &mut Cursor<&[u8]>,
    depth: usize,
) -> io::Result<BTreeMap<String, VdfValue>> {
    if depth > MAX_OBJECT_DEPTH {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "VDF objects are nested too deeply",
        ));
    }
    let mut fields = BTreeMap::new();
    let mut marker = [0u8; 1];

    loop {
        cursor.read_exact(&mut marker)?;
        match marker[0] {
            TYPE_END => return Ok(fields),
            TYPE_STRING => {
                let key = read_null_term_string(cursor)?;
                let val = read_null_term_string(cursor)?;
                fields.insert(key, VdfValue::String(val));
            }
            TYPE_INT32 => {
                let key = read_null_term_string(cursor)?;
                let val = read_int32_le(cursor)?;
                fields.insert(key, VdfValue::Int(val));
            }
            TYPE_OBJECT => {
                let key = read_null_term_string(cursor)?;
                let sub = read_object_fields(cursor, depth + 1)?;
                fields.insert(key, VdfValue::Object(sub));
            }
            other => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("Unknown VDF type marker: 0x{other:02x}"),
                ));
            }
        }
    }
}

fn write_shortcut_fields(buf: &mut Vec<u8>, s: &SteamShortcut) -> io::Result<()> {
    write_int32_field(buf, "appid", s.app_id)?;
    write_string_field(buf, "AppName", &s.app_name)?;
    write_string_field(buf, "Exe", &s.exe)?;
    write_string_field(buf, "StartDir", &s.start_dir)?;
    write_string_field(buf, "icon", &s.icon)?;
    write_string_field(buf, "ShortcutPath", &s.shortcut_path)?;
    write_string_field(buf, "LaunchOptions", &s.launch_options)?;
    write_int32_field(buf, "IsHidden", if s.is_hidden { 1 } else { 0 })?;
    write_int32_field(
        buf,
        "AllowDesktopConfig",
        if s.allow_desktop_config { 1 } else { 0 },
    )?;
    write_int32_field(buf, "AllowOverlay", if s.allow_overlay { 1 } else { 0 })?;
    write_int32_field(buf, "OpenVR", if s.open_vr { 1 } else { 0 })?;
    write_int32_field(buf, "Devkit", if s.devkit { 1 } else { 0 })?;
    write_string_field(buf, "DevkitGameID", &s.devkit_game_id)?;
    write_int32_field(buf, "DevkitOverrideAppID", s.devkit_override_app_id)?;
    write_int32_field(buf, "LastPlayTime", s.last_play_time)?;
    write_string_field(buf, "FlatpakAppID", &s.flatpak_app_id)?;
    write_string_field(buf, "sortas", &s.sort_as)?;

    // Tags sub-object
    buf.push(TYPE_OBJECT);
    write_null_term_string(buf, "tags")?;
    for (key, val) in &s.tags {
        write_string_field(buf, key, val)?;
    }
    buf.push(TYPE_END);

    // Extra fields
    for (key, val) in &s.extra_fields {
        write_extra_field(buf, key, val)?;
    }

    Ok(())
}

fn write_extra_field(buf: &mut Vec<u8>, key: &str, val: &VdfValue) -> io::Result<()> {
    match val {
        VdfValue::String(s) => write_string_field(buf, key, s),
        VdfValue::Int(i) => write_int32_field(buf, key, *i),
        VdfValue::Object(map) => {
            buf.push(TYPE_OBJECT);
            write_null_term_string(buf, key)?;
            for (sub_key, sub_val) in map {
                write_extra_field(buf, sub_key, sub_val)?;
            }
            buf.push(TYPE_END);
            Ok(())
        }
    }
}

/// Los valores de `tags` que no son cadenas se descartan (comportamiento histórico;
/// conservarlos exigiría cambiar el formato escrito).
fn build_shortcut(mut fields: BTreeMap<String, VdfValue>) -> SteamShortcut {
    fn string_field(
        fields: &mut BTreeMap<String, VdfValue>,
        name: &str,
        alias: Option<&str>,
    ) -> String {
        // Ambas claves se retiran para que ninguna quede como campo extra.
        let primary = fields.remove(name);
        let secondary = alias.and_then(|a| fields.remove(a));
        match (primary, secondary) {
            (Some(VdfValue::String(v)), _) | (_, Some(VdfValue::String(v))) => v,
            _ => String::new(),
        }
    }

    fn int_field(fields: &mut BTreeMap<String, VdfValue>, name: &str, default: i32) -> i32 {
        match fields.remove(name) {
            Some(VdfValue::Int(val)) => val,
            _ => default,
        }
    }

    let tags = match fields.remove("tags") {
        Some(VdfValue::Object(map)) => map
            .into_iter()
            .filter_map(|(k, v)| match v {
                VdfValue::String(s) => Some((k, s)),
                _ => None,
            })
            .collect(),
        _ => BTreeMap::new(),
    };

    let f = &mut fields;
    let app_id = int_field(f, "appid", 0);
    let app_name = string_field(f, "AppName", Some("appname"));
    let exe = string_field(f, "Exe", Some("exe"));
    let start_dir = string_field(f, "StartDir", Some("startdir"));
    let icon = string_field(f, "icon", None);
    let shortcut_path = string_field(f, "ShortcutPath", None);
    let launch_options = string_field(f, "LaunchOptions", None);
    let is_hidden = int_field(f, "IsHidden", 0) != 0;
    let allow_desktop_config = int_field(f, "AllowDesktopConfig", 1) != 0;
    let allow_overlay = int_field(f, "AllowOverlay", 1) != 0;
    let open_vr = int_field(f, "OpenVR", 0) != 0;
    let devkit = int_field(f, "Devkit", 0) != 0;
    let devkit_game_id = string_field(f, "DevkitGameID", None);
    let devkit_override_app_id = int_field(f, "DevkitOverrideAppID", 0);
    let last_play_time = int_field(f, "LastPlayTime", 0);
    let flatpak_app_id = string_field(f, "FlatpakAppID", None);
    let sort_as = string_field(f, "sortas", None);

    SteamShortcut {
        app_id,
        app_name,
        exe,
        start_dir,
        icon,
        shortcut_path,
        launch_options,
        is_hidden,
        allow_desktop_config,
        allow_overlay,
        open_vr,
        devkit,
        devkit_game_id,
        devkit_override_app_id,
        last_play_time,
        flatpak_app_id,
        sort_as,
        tags,
        extra_fields: fields,
    }
}
