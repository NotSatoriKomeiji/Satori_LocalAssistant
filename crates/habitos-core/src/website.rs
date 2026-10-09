//! A website target is an HTTPS origin, never a document URL or shell argument.
use crate::{rule, Result};
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Visit {
    pub origin: String,
    pub at: i64,
}
pub fn origin(input: &str) -> Result<String> {
    if input.len() > 2048 || input.chars().any(|c| c.is_control()) {
        return Err(rule("网址格式无效"));
    }
    let u = url::Url::parse(input).map_err(|_| rule("请输入完整的HTTPS网址"))?;
    if u.scheme() != "https"
        || !u.username().is_empty()
        || u.password().is_some()
        || u.port().is_some()
    {
        return Err(rule("只支持无凭据、默认端口的HTTPS站点"));
    }
    let host = u.host_str().ok_or_else(|| rule("网址缺少域名"))?;
    if host.parse::<std::net::IpAddr>().is_ok()
        || !host.contains('.')
        || host.ends_with('.')
        || host.ends_with(".local")
        || host.ends_with(".localhost")
    {
        return Err(rule("本地地址不参加网站推荐"));
    }
    Ok(format!("https://{host}/"))
}
pub fn id(origin: &str) -> String {
    format!("web_{}", crate::platform::identity(origin))
}
pub fn browser(name: &str) -> bool {
    ["chrome.exe", "msedge.exe", "browser.exe（模拟）"]
        .iter()
        .any(|x| name.eq_ignore_ascii_case(x))
}
pub fn read_message(reader: &mut impl Read) -> Result<Visit> {
    let mut len = [0; 4];
    reader
        .read_exact(&mut len)
        .map_err(|_| rule("浏览器消息头不完整"))?;
    let len = u32::from_le_bytes(len) as usize;
    if len == 0 || len > 4096 {
        return Err(rule("浏览器消息超出范围"));
    }
    let mut bytes = vec![0; len];
    reader
        .read_exact(&mut bytes)
        .map_err(|_| rule("浏览器消息不完整"))?;
    let mut visit: Visit = serde_json::from_slice(&bytes)?;
    visit.origin = origin(&visit.origin)?;
    Ok(visit)
}
pub fn reply(writer: &mut impl Write, accepted: bool) -> std::io::Result<()> {
    let bytes = if accepted {
        b"{\"accepted\":true}".as_slice()
    } else {
        b"{\"accepted\":false}".as_slice()
    };
    writer.write_all(&(bytes.len() as u32).to_le_bytes())?;
    writer.write_all(bytes)?;
    writer.flush()
}
