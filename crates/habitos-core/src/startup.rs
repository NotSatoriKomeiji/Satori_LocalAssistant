pub fn command(executable: &str) -> crate::Result<String> {
    if executable.is_empty() || executable.contains(['\0', '"']) {
        return Err(crate::rule("启动路径无效"));
    }
    let value = format!("\"{executable}\" --background");
    if value.encode_utf16().count() > 260 {
        return Err(crate::rule("路径过长，请将程序移到较短目录后开启自启动"));
    }
    Ok(value)
}
