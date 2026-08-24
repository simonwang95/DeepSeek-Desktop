use crate::models::{AppConfig, CommandConfig};
use std::path::{Component, Path};

pub fn validate_config(config: &AppConfig) -> Result<(), String> {
    validate_source_dir(&config.harness.source_dir)?;
    validate_repo_url(&config.harness.repo_url)?;
    validate_port(config.harness.port)?;
    validate_command(&config.harness.start_command)?;
    validate_command(&config.harness.install_command)?;
    validate_command(&config.harness.build_command)?;
    if config.harness.branch.trim().is_empty() || config.update.remote_name.trim().is_empty() {
        return Err("分支名和远程仓库名不能为空".into());
    }
    for path in &config.harness.clean_paths {
        validate_relative_path(path)?;
    }
    validate_lm_url(&config.lm_studio.api_url)
}

pub fn validate_source_dir(value: &str) -> Result<(), String> {
    let value = value.trim();
    if value.is_empty() || value.contains('\0') {
        return Err("Harness 源码目录不能为空或包含非法字符".into());
    }
    let path = Path::new(value);
    if path.parent().is_none()
        || path == Path::new("/")
        || path.components().count() <= 1 && path.is_absolute()
    {
        return Err("为了避免误操作，源码目录不能是文件系统根目录".into());
    }
    Ok(())
}

pub fn validate_repo_url(value: &str) -> Result<(), String> {
    let value = value.trim();
    let accepted = (value.starts_with("https://")
        || value.starts_with("http://")
        || value.starts_with("ssh://")
        || value.starts_with("git@"))
        && !value.chars().any(|ch| ch.is_whitespace() || ch == '\0');
    if accepted {
        Ok(())
    } else {
        Err("Git URL 需要使用 HTTPS、SSH 或 git@host:path 格式".into())
    }
}

pub fn validate_lm_url(value: &str) -> Result<(), String> {
    let value = value.trim();
    if !(value.starts_with("http://") || value.starts_with("https://"))
        || value.contains('@')
        || value.chars().any(|ch| ch.is_whitespace() || ch == '\0')
    {
        return Err("LM Studio API 地址必须是不含凭据的 HTTP(S) URL".into());
    }
    Ok(())
}

pub fn validate_port(port: u16) -> Result<(), String> {
    if port == 0 {
        Err("端口必须是 1 到 65535 之间的整数".into())
    } else {
        Ok(())
    }
}

pub fn validate_command(command: &CommandConfig) -> Result<(), String> {
    if command.program.trim().is_empty() || command.program.contains('\0') {
        return Err("可执行文件不能为空或包含非法字符".into());
    }
    if command.args.iter().any(|arg| arg.contains('\0')) {
        return Err("命令参数包含非法字符".into());
    }
    Ok(())
}

pub fn validate_relative_path(value: &str) -> Result<(), String> {
    let path = Path::new(value);
    if value.trim().is_empty()
        || path.is_absolute()
        || path.components().any(|part| {
            matches!(
                part,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(format!("清理路径必须是源码目录内的相对路径: {value}"));
    }
    Ok(())
}

pub fn redact(value: &str) -> String {
    let mut result = value.to_string();
    for key in [
        "Authorization",
        "authorization",
        "api_key",
        "apiKey",
        "token",
        "password",
        "secret",
    ] {
        let mut start = 0;
        while let Some(offset) = result[start..].find(key) {
            let absolute = start + offset;
            if let Some(colon) = result[absolute..].find(':') {
                let value_start = absolute + colon + 1;
                let end = result[value_start..]
                    .find(',')
                    .map(|pos| value_start + pos)
                    .unwrap_or(result.len());
                result.replace_range(value_start..end, " [REDACTED]");
                start = (value_start + 12).min(result.len());
            } else {
                break;
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::{redact, validate_relative_path};

    #[test]
    fn redacts_common_secret_fields() {
        assert_eq!(
            redact("Authorization: Bearer abc123"),
            "Authorization: [REDACTED]"
        );
    }

    #[test]
    fn rejects_paths_that_escape_checkout() {
        assert!(validate_relative_path("../dist").is_err());
        assert!(validate_relative_path("dist").is_ok());
    }
}
