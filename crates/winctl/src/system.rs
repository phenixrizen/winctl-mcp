use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Error)]
#[error("{message}")]
pub struct SystemError {
    pub code: String,
    pub message: String,
    #[serde(default)]
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ClipboardText {
    pub text: Option<String>,
    pub format: String,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ClipboardWriteResult {
    pub written: bool,
    pub chars: usize,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RegistryHive {
    CurrentUser,
    LocalMachine,
    ClassesRoot,
    Users,
    CurrentConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RegistryValueKind {
    String,
    ExpandString,
    Dword,
    Qword,
    MultiString,
    Binary,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RegistryValue {
    pub name: String,
    pub kind: String,
    pub raw_type: u32,
    pub data: Value,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RegistryKeyListing {
    pub hive: RegistryHive,
    pub path: String,
    pub subkeys: Vec<String>,
    pub values: Vec<RegistryValue>,
    pub warnings: Vec<String>,
}

pub fn clipboard_read_text() -> Result<ClipboardText, SystemError> {
    #[cfg(windows)]
    {
        windows_impl::clipboard_read_text()
    }

    #[cfg(not(windows))]
    {
        Err(SystemError {
            code: "unsupported_platform".into(),
            message: "clipboard.read requires Windows runtime".into(),
            warnings: vec![],
        })
    }
}

pub fn clipboard_write_text(text: &str) -> Result<ClipboardWriteResult, SystemError> {
    #[cfg(windows)]
    {
        windows_impl::clipboard_write_text(text)
    }

    #[cfg(not(windows))]
    {
        let _ = text;
        Err(SystemError {
            code: "unsupported_platform".into(),
            message: "clipboard.write requires Windows runtime".into(),
            warnings: vec![],
        })
    }
}

pub fn registry_list(
    hive: RegistryHive,
    path: &str,
    include_values: bool,
) -> Result<RegistryKeyListing, SystemError> {
    #[cfg(windows)]
    {
        windows_impl::registry_list(hive, path, include_values)
    }

    #[cfg(not(windows))]
    {
        let _ = (hive, path, include_values);
        Err(SystemError {
            code: "unsupported_platform".into(),
            message: "registry.list requires Windows runtime".into(),
            warnings: vec![],
        })
    }
}

pub fn registry_read(
    hive: RegistryHive,
    path: &str,
    name: Option<&str>,
) -> Result<RegistryValue, SystemError> {
    #[cfg(windows)]
    {
        windows_impl::registry_read(hive, path, name)
    }

    #[cfg(not(windows))]
    {
        let _ = (hive, path, name);
        Err(SystemError {
            code: "unsupported_platform".into(),
            message: "registry.read requires Windows runtime".into(),
            warnings: vec![],
        })
    }
}

pub fn registry_write(
    hive: RegistryHive,
    path: &str,
    name: Option<&str>,
    kind: RegistryValueKind,
    data: &Value,
) -> Result<RegistryValue, SystemError> {
    #[cfg(windows)]
    {
        windows_impl::registry_write(hive, path, name, kind, data)
    }

    #[cfg(not(windows))]
    {
        let _ = (hive, path, name, kind, data);
        Err(SystemError {
            code: "unsupported_platform".into(),
            message: "registry.write requires Windows runtime".into(),
            warnings: vec![],
        })
    }
}

pub fn registry_delete(
    hive: RegistryHive,
    path: &str,
    name: Option<&str>,
) -> Result<(), SystemError> {
    #[cfg(windows)]
    {
        windows_impl::registry_delete(hive, path, name)
    }

    #[cfg(not(windows))]
    {
        let _ = (hive, path, name);
        Err(SystemError {
            code: "unsupported_platform".into(),
            message: "registry.delete requires Windows runtime".into(),
            warnings: vec![],
        })
    }
}

#[cfg(windows)]
mod windows_impl {
    use std::ffi::{OsStr, OsString};
    use std::mem::size_of;
    use std::os::windows::ffi::{OsStrExt, OsStringExt};

    use serde_json::Value;
    use windows::core::{PCWSTR, PWSTR};
    use windows::Win32::Foundation::{
        GlobalFree, ERROR_NO_MORE_ITEMS, ERROR_SUCCESS, HANDLE, HGLOBAL,
    };
    use windows::Win32::System::DataExchange::{
        CloseClipboard, EmptyClipboard, GetClipboardData, IsClipboardFormatAvailable,
        OpenClipboard, SetClipboardData,
    };
    use windows::Win32::System::Memory::{
        GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock, GMEM_MOVEABLE,
    };
    use windows::Win32::System::Registry::{
        RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegEnumKeyExW, RegEnumValueW, RegOpenKeyExW,
        RegQueryValueExW, RegSetValueExW, HKEY, HKEY_CLASSES_ROOT, HKEY_CURRENT_CONFIG,
        HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, HKEY_USERS, KEY_READ, KEY_WRITE, REG_BINARY,
        REG_CREATE_KEY_DISPOSITION, REG_DWORD, REG_EXPAND_SZ, REG_MULTI_SZ,
        REG_OPTION_NON_VOLATILE, REG_QWORD, REG_SZ, REG_VALUE_TYPE,
    };

    use super::{
        ClipboardText, ClipboardWriteResult, RegistryHive, RegistryKeyListing, RegistryValue,
        RegistryValueKind, SystemError,
    };

    const CF_UNICODETEXT: u32 = 13;

    pub(super) fn clipboard_read_text() -> Result<ClipboardText, SystemError> {
        unsafe { OpenClipboard(None) }.map_err(|error| SystemError {
            code: "clipboard_open_failed".into(),
            message: format!("failed to open clipboard: {error}"),
            warnings: vec![],
        })?;
        let result = read_open_clipboard();
        let _ = unsafe { CloseClipboard() };
        result
    }

    fn read_open_clipboard() -> Result<ClipboardText, SystemError> {
        if unsafe { IsClipboardFormatAvailable(CF_UNICODETEXT) }.is_err() {
            return Ok(ClipboardText {
                text: None,
                format: "unicode_text".into(),
                warnings: vec!["clipboard does not contain CF_UNICODETEXT".into()],
            });
        }
        let handle = unsafe { GetClipboardData(CF_UNICODETEXT) }.map_err(|error| SystemError {
            code: "clipboard_data_unavailable".into(),
            message: format!("failed to get clipboard text: {error}"),
            warnings: vec![],
        })?;
        let hglobal = HGLOBAL(handle.0);
        let ptr = unsafe { GlobalLock(hglobal) } as *const u16;
        if ptr.is_null() {
            return Err(SystemError {
                code: "clipboard_lock_failed".into(),
                message: "failed to lock clipboard text memory".into(),
                warnings: vec![],
            });
        }
        let bytes = unsafe { GlobalSize(hglobal) };
        let units = bytes / size_of::<u16>();
        let slice = unsafe { std::slice::from_raw_parts(ptr, units) };
        let end = slice
            .iter()
            .position(|unit| *unit == 0)
            .unwrap_or(slice.len());
        let text = String::from_utf16_lossy(&slice[..end]);
        let _ = unsafe { GlobalUnlock(hglobal) };
        Ok(ClipboardText {
            text: Some(text),
            format: "unicode_text".into(),
            warnings: vec![],
        })
    }

    pub(super) fn clipboard_write_text(text: &str) -> Result<ClipboardWriteResult, SystemError> {
        let mut units: Vec<u16> = OsStr::new(text).encode_wide().collect();
        units.push(0);
        let bytes = units.len() * size_of::<u16>();
        let hglobal =
            unsafe { GlobalAlloc(GMEM_MOVEABLE, bytes) }.map_err(|error| SystemError {
                code: "clipboard_alloc_failed".into(),
                message: format!("failed to allocate clipboard memory: {error}"),
                warnings: vec![],
            })?;
        let ptr = unsafe { GlobalLock(hglobal) } as *mut u16;
        if ptr.is_null() {
            let _ = unsafe { GlobalFree(Some(hglobal)) };
            return Err(SystemError {
                code: "clipboard_lock_failed".into(),
                message: "failed to lock clipboard write memory".into(),
                warnings: vec![],
            });
        }
        unsafe { std::ptr::copy_nonoverlapping(units.as_ptr(), ptr, units.len()) };
        let _ = unsafe { GlobalUnlock(hglobal) };

        unsafe { OpenClipboard(None) }.map_err(|error| {
            let _ = unsafe { GlobalFree(Some(hglobal)) };
            SystemError {
                code: "clipboard_open_failed".into(),
                message: format!("failed to open clipboard: {error}"),
                warnings: vec![],
            }
        })?;
        let result = unsafe { EmptyClipboard() }
            .map_err(|error| SystemError {
                code: "clipboard_empty_failed".into(),
                message: format!("failed to empty clipboard: {error}"),
                warnings: vec![],
            })
            .and_then(|_| {
                unsafe { SetClipboardData(CF_UNICODETEXT, Some(HANDLE(hglobal.0))) }.map_err(
                    |error| SystemError {
                        code: "clipboard_set_failed".into(),
                        message: format!("failed to set clipboard text: {error}"),
                        warnings: vec![],
                    },
                )
            });
        let _ = unsafe { CloseClipboard() };
        result.map(|_| ClipboardWriteResult {
            written: true,
            chars: text.chars().count(),
            warnings: vec![],
        })
    }

    pub(super) fn registry_list(
        hive: RegistryHive,
        path: &str,
        include_values: bool,
    ) -> Result<RegistryKeyListing, SystemError> {
        let key = open_key(hive.clone(), path, KEY_READ)?;
        let mut subkeys = Vec::new();
        let mut index = 0u32;
        loop {
            let mut name = vec![0u16; 512];
            let mut len = name.len() as u32;
            let status = unsafe {
                RegEnumKeyExW(
                    key,
                    index,
                    Some(PWSTR(name.as_mut_ptr())),
                    &mut len,
                    None,
                    None,
                    None,
                    None,
                )
            };
            if status == ERROR_NO_MORE_ITEMS {
                break;
            }
            if status != ERROR_SUCCESS {
                let _ = unsafe { RegCloseKey(key) };
                return Err(reg_error("registry_enum_key_failed", status.0, path));
            }
            subkeys.push(String::from_utf16_lossy(&name[..len as usize]));
            index += 1;
        }
        let values = if include_values {
            enum_values(key, path)?
        } else {
            Vec::new()
        };
        let _ = unsafe { RegCloseKey(key) };
        Ok(RegistryKeyListing {
            hive,
            path: path.into(),
            subkeys,
            values,
            warnings: vec![],
        })
    }

    pub(super) fn registry_read(
        hive: RegistryHive,
        path: &str,
        name: Option<&str>,
    ) -> Result<RegistryValue, SystemError> {
        let key = open_key(hive, path, KEY_READ)?;
        let value = query_value(key, name.unwrap_or(""), path);
        let _ = unsafe { RegCloseKey(key) };
        value
    }

    pub(super) fn registry_write(
        hive: RegistryHive,
        path: &str,
        name: Option<&str>,
        kind: RegistryValueKind,
        data: &Value,
    ) -> Result<RegistryValue, SystemError> {
        let key = create_key(hive.clone(), path)?;
        let (raw_kind, bytes) = encode_registry_value(&kind, data)?;
        let name_wide = wide_null(name.unwrap_or(""));
        let status = unsafe {
            RegSetValueExW(
                key,
                PCWSTR(name_wide.as_ptr()),
                None,
                raw_kind,
                Some(&bytes),
            )
        };
        if status != ERROR_SUCCESS {
            let _ = unsafe { RegCloseKey(key) };
            return Err(reg_error("registry_set_value_failed", status.0, path));
        }
        let value = query_value(key, name.unwrap_or(""), path);
        let _ = unsafe { RegCloseKey(key) };
        value
    }

    pub(super) fn registry_delete(
        hive: RegistryHive,
        path: &str,
        name: Option<&str>,
    ) -> Result<(), SystemError> {
        let key = open_key(hive, path, KEY_WRITE)?;
        let name_wide = wide_null(name.unwrap_or(""));
        let status = unsafe { RegDeleteValueW(key, PCWSTR(name_wide.as_ptr())) };
        let _ = unsafe { RegCloseKey(key) };
        if status == ERROR_SUCCESS {
            Ok(())
        } else {
            Err(reg_error("registry_delete_value_failed", status.0, path))
        }
    }

    fn enum_values(key: HKEY, path: &str) -> Result<Vec<RegistryValue>, SystemError> {
        let mut values = Vec::new();
        let mut index = 0u32;
        loop {
            let mut name = vec![0u16; 512];
            let mut len = name.len() as u32;
            let mut raw_type = 0u32;
            let status = unsafe {
                RegEnumValueW(
                    key,
                    index,
                    Some(PWSTR(name.as_mut_ptr())),
                    &mut len,
                    None,
                    Some(&mut raw_type),
                    None,
                    None,
                )
            };
            if status == ERROR_NO_MORE_ITEMS {
                break;
            }
            if status != ERROR_SUCCESS {
                return Err(reg_error("registry_enum_value_failed", status.0, path));
            }
            let value_name = String::from_utf16_lossy(&name[..len as usize]);
            values.push(query_value(key, &value_name, path)?);
            index += 1;
        }
        Ok(values)
    }

    fn query_value(key: HKEY, name: &str, path: &str) -> Result<RegistryValue, SystemError> {
        let name_wide = wide_null(name);
        let mut raw_type = REG_VALUE_TYPE(0);
        let mut bytes_len = 0u32;
        let status = unsafe {
            RegQueryValueExW(
                key,
                PCWSTR(name_wide.as_ptr()),
                None,
                Some(&mut raw_type),
                None,
                Some(&mut bytes_len),
            )
        };
        if status != ERROR_SUCCESS {
            return Err(reg_error("registry_query_value_failed", status.0, path));
        }
        let mut bytes = vec![0u8; bytes_len as usize];
        let status = unsafe {
            RegQueryValueExW(
                key,
                PCWSTR(name_wide.as_ptr()),
                None,
                Some(&mut raw_type),
                Some(bytes.as_mut_ptr()),
                Some(&mut bytes_len),
            )
        };
        if status != ERROR_SUCCESS {
            return Err(reg_error("registry_query_value_failed", status.0, path));
        }
        bytes.truncate(bytes_len as usize);
        Ok(decode_registry_value(name, raw_type, &bytes))
    }

    fn decode_registry_value(name: &str, raw_type: REG_VALUE_TYPE, bytes: &[u8]) -> RegistryValue {
        let (kind, data) = if raw_type == REG_SZ || raw_type == REG_EXPAND_SZ {
            (
                if raw_type == REG_SZ {
                    "string"
                } else {
                    "expand_string"
                },
                Value::String(decode_utf16_z(bytes)),
            )
        } else if raw_type == REG_MULTI_SZ {
            ("multi_string", Value::Array(decode_multi_string(bytes)))
        } else if raw_type == REG_DWORD && bytes.len() >= 4 {
            (
                "dword",
                Value::Number(u32::from_le_bytes(bytes[..4].try_into().unwrap()).into()),
            )
        } else if raw_type == REG_QWORD && bytes.len() >= 8 {
            (
                "qword",
                Value::Number(u64::from_le_bytes(bytes[..8].try_into().unwrap()).into()),
            )
        } else if raw_type == REG_BINARY {
            (
                "binary",
                Value::Array(bytes.iter().map(|byte| (*byte).into()).collect()),
            )
        } else {
            (
                "unknown",
                Value::Array(bytes.iter().map(|byte| (*byte).into()).collect()),
            )
        };
        RegistryValue {
            name: name.into(),
            kind: kind.into(),
            raw_type: raw_type.0,
            data,
            warnings: vec![],
        }
    }

    fn encode_registry_value(
        kind: &RegistryValueKind,
        data: &Value,
    ) -> Result<(REG_VALUE_TYPE, Vec<u8>), SystemError> {
        match kind {
            RegistryValueKind::String | RegistryValueKind::ExpandString => {
                let text = data.as_str().ok_or_else(|| SystemError {
                    code: "invalid_registry_value".into(),
                    message: "string registry values require JSON string data".into(),
                    warnings: vec![],
                })?;
                let raw_type = if *kind == RegistryValueKind::String {
                    REG_SZ
                } else {
                    REG_EXPAND_SZ
                };
                Ok((raw_type, encode_utf16_z(text)))
            }
            RegistryValueKind::Dword => {
                let number = data.as_u64().ok_or_else(|| SystemError {
                    code: "invalid_registry_value".into(),
                    message: "dword registry values require JSON number data".into(),
                    warnings: vec![],
                })?;
                Ok((REG_DWORD, (number as u32).to_le_bytes().to_vec()))
            }
            RegistryValueKind::Qword => {
                let number = data.as_u64().ok_or_else(|| SystemError {
                    code: "invalid_registry_value".into(),
                    message: "qword registry values require JSON number data".into(),
                    warnings: vec![],
                })?;
                Ok((REG_QWORD, number.to_le_bytes().to_vec()))
            }
            RegistryValueKind::MultiString => {
                let values = data.as_array().ok_or_else(|| SystemError {
                    code: "invalid_registry_value".into(),
                    message: "multi_string registry values require JSON string array data".into(),
                    warnings: vec![],
                })?;
                let mut bytes = Vec::new();
                for value in values {
                    let text = value.as_str().ok_or_else(|| SystemError {
                        code: "invalid_registry_value".into(),
                        message: "multi_string registry values require JSON string array data"
                            .into(),
                        warnings: vec![],
                    })?;
                    bytes.extend(encode_utf16_z(text));
                }
                bytes.extend(0u16.to_le_bytes());
                Ok((REG_MULTI_SZ, bytes))
            }
            RegistryValueKind::Binary => {
                let values = data.as_array().ok_or_else(|| SystemError {
                    code: "invalid_registry_value".into(),
                    message: "binary registry values require JSON byte array data".into(),
                    warnings: vec![],
                })?;
                let mut bytes = Vec::with_capacity(values.len());
                for value in values {
                    let byte = value.as_u64().ok_or_else(|| SystemError {
                        code: "invalid_registry_value".into(),
                        message: "binary registry values require JSON byte array data".into(),
                        warnings: vec![],
                    })?;
                    if byte > u8::MAX as u64 {
                        return Err(SystemError {
                            code: "invalid_registry_value".into(),
                            message: "binary registry byte values must be 0..255".into(),
                            warnings: vec![],
                        });
                    }
                    bytes.push(byte as u8);
                }
                Ok((REG_BINARY, bytes))
            }
        }
    }

    fn open_key(
        hive: RegistryHive,
        path: &str,
        access: windows::Win32::System::Registry::REG_SAM_FLAGS,
    ) -> Result<HKEY, SystemError> {
        let mut key = HKEY::default();
        let path_wide = wide_null(path);
        let status = unsafe {
            RegOpenKeyExW(
                hive_root(&hive),
                PCWSTR(path_wide.as_ptr()),
                Some(0),
                access,
                &mut key,
            )
        };
        if status == ERROR_SUCCESS {
            Ok(key)
        } else {
            Err(reg_error("registry_open_failed", status.0, path))
        }
    }

    fn create_key(hive: RegistryHive, path: &str) -> Result<HKEY, SystemError> {
        let mut key = HKEY::default();
        let mut disposition = REG_CREATE_KEY_DISPOSITION(0);
        let path_wide = wide_null(path);
        let status = unsafe {
            RegCreateKeyExW(
                hive_root(&hive),
                PCWSTR(path_wide.as_ptr()),
                Some(0),
                None,
                REG_OPTION_NON_VOLATILE,
                KEY_WRITE | KEY_READ,
                None,
                &mut key,
                Some(&mut disposition),
            )
        };
        if status == ERROR_SUCCESS {
            Ok(key)
        } else {
            Err(reg_error("registry_create_failed", status.0, path))
        }
    }

    fn hive_root(hive: &RegistryHive) -> HKEY {
        match hive {
            RegistryHive::CurrentUser => HKEY_CURRENT_USER,
            RegistryHive::LocalMachine => HKEY_LOCAL_MACHINE,
            RegistryHive::ClassesRoot => HKEY_CLASSES_ROOT,
            RegistryHive::Users => HKEY_USERS,
            RegistryHive::CurrentConfig => HKEY_CURRENT_CONFIG,
        }
    }

    fn reg_error(code: &str, win32: u32, path: &str) -> SystemError {
        SystemError {
            code: code.into(),
            message: format!("{code} for registry path {path}: win32 error {win32}"),
            warnings: vec![],
        }
    }

    fn wide_null(value: &str) -> Vec<u16> {
        OsStr::new(value).encode_wide().chain(Some(0)).collect()
    }

    fn encode_utf16_z(value: &str) -> Vec<u8> {
        OsStr::new(value)
            .encode_wide()
            .chain(Some(0))
            .flat_map(u16::to_le_bytes)
            .collect()
    }

    fn decode_utf16_z(bytes: &[u8]) -> String {
        let mut units = bytes_to_u16_units(bytes);
        if let Some(end) = units.iter().position(|unit| *unit == 0) {
            units.truncate(end);
        }
        OsString::from_wide(&units).to_string_lossy().to_string()
    }

    fn decode_multi_string(bytes: &[u8]) -> Vec<Value> {
        let units = bytes_to_u16_units(bytes);
        let mut values = Vec::new();
        let mut start = 0usize;
        for (index, unit) in units.iter().enumerate() {
            if *unit != 0 {
                continue;
            }
            if index == start {
                break;
            }
            values.push(Value::String(
                OsString::from_wide(&units[start..index])
                    .to_string_lossy()
                    .to_string(),
            ));
            start = index + 1;
        }
        values
    }

    fn bytes_to_u16_units(bytes: &[u8]) -> Vec<u16> {
        bytes
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_hive_serializes_as_snake_case() {
        assert_eq!(
            serde_json::to_value(RegistryHive::CurrentUser).unwrap(),
            Value::String("current_user".into())
        );
    }
}
