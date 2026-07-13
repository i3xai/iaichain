//! 在线升级：通过 GitHub Releases 拉取新版本，SHA256 校验后原子替换当前二进制。
//!
//! 资产命名约定（与发布流水线对齐）：
//!   `iai-v<TAG>-<TARGET>.tar.gz`
//!   `iai-v<TAG>-<TARGET>.tar.gz.sha256`（可选，缺失时仅打印警告）
//!
//! TARGET ∈ { linux-x86_64, linux-aarch64, macos-x86_64, macos-aarch64, windows-x86_64 }
//!
//! 流程：fetch latest → 匹配 asset → 询问 → 下载 → SHA256 → 解压 → 备份 → 替换 → 重启 systemd。
//! 失败时旧二进制保留为 `iai.bak.<时间戳>`，可用 `mv` 回滚。

use anyhow::{anyhow, bail, Context, Result};
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::process::Command;

const REPO_OWNER: &str = "i3xai";
const REPO_NAME: &str = "iaichain";
const API_LATEST: &str = "https://api.github.com/repos/i3xai/iaichain/releases/latest";
const API_TAG_FMT: &str = "https://api.github.com/repos/i3xai/iaichain/releases/tags/";

#[derive(Deserialize)]
struct ReleaseResp {
    tag_name: String,
    #[allow(dead_code)]
    name: String,
    #[serde(default)]
    assets: Vec<AssetResp>,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    published_at: Option<String>,
}

#[derive(Deserialize, Clone)]
struct AssetResp {
    name: String,
    browser_download_url: String,
    #[allow(dead_code)]
    size: u64,
}

pub fn current_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// 当前 host 的目标三元组（与 release asset 命名对齐）。
pub fn host_target_triple() -> &'static str {
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    {
        "linux-x86_64"
    }
    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    {
        "linux-aarch64"
    }
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    {
        "macos-x86_64"
    }
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        "macos-aarch64"
    }
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    {
        "windows-x86_64"
    }
    #[cfg(not(any(
        all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64")),
        all(target_os = "macos", any(target_arch = "x86_64", target_arch = "aarch64")),
        all(target_os = "windows", target_arch = "x86_64")
    )))]
    {
        "unknown"
    }
}

fn asset_name(tag: &str, target: &str) -> String {
    let v = tag.trim_start_matches('v');
    format!("iai-v{}-{}.tar.gz", v, target)
}

fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent(format!("iai-cli/{}", current_version()))
        // 大文件 + 弱网：拉长超时；重定向默认跟随（GitHub → release-assets）
        .timeout(std::time::Duration::from_secs(180))
        .connect_timeout(std::env::var("IAI_CONNECT_TIMEOUT")
            .ok()
            .and_then(|s| s.parse().ok())
            .map(std::time::Duration::from_secs)
            .unwrap_or_else(|| std::time::Duration::from_secs(20)))
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()
        .expect("构建 HTTP client 失败")
}

/// 官网静态镜像（publish.sh 会同步 dist/ 到此路径）。
const OFFICIAL_MIRROR: &str = "https://iaiaiai.ai/releases";

/// 为 GitHub 资源生成候选下载 URL（官网镜像优先，规避 github.com TLS 不稳）。
fn download_candidates(github_url: &str, asset_file: &str) -> Vec<String> {
    let mut urls = Vec::new();
    // 自定义前缀：IAI_DOWNLOAD_MIRROR=https://ghfast.top/https://github.com
    if let Ok(mirror) = std::env::var("IAI_DOWNLOAD_MIRROR") {
        let m = mirror.trim_end_matches('/');
        if !m.is_empty() {
            if github_url.starts_with("https://github.com/") {
                urls.push(format!(
                    "{m}/{}",
                    github_url.trim_start_matches("https://github.com/")
                ));
            } else {
                urls.push(format!("{m}/{asset_file}"));
            }
        }
    }
    // 官网静态镜像（国内可达）
    urls.push(format!("{OFFICIAL_MIRROR}/{asset_file}"));
    // 公共 GitHub 加速
    if github_url.starts_with("https://github.com/") {
        let rest = github_url.trim_start_matches("https://github.com/");
        urls.push(format!("https://ghfast.top/https://github.com/{rest}"));
        urls.push(format!("https://ghproxy.net/https://github.com/{rest}"));
    }
    // 直连 GitHub（海外 / 网络正常时）
    urls.push(github_url.to_string());
    let mut seen = std::collections::HashSet::new();
    urls.retain(|u| seen.insert(u.clone()));
    urls
}

async fn download_to_with_mirrors(primary: &str, dst: &Path, asset_file: Option<&str>) -> Result<()> {
    let name = asset_file.map(|s| s.to_string()).unwrap_or_else(|| {
        primary
            .rsplit('/')
            .next()
            .unwrap_or("download.bin")
            .to_string()
    });
    let candidates = download_candidates(primary, &name);
    let mut last_err = anyhow!("无可用下载源");
    for (i, url) in candidates.iter().enumerate() {
        for attempt in 1..=2 {
            if i > 0 || attempt > 1 {
                println!("  ↻ 尝试 {} (#{attempt})", url);
            } else {
                println!("  → {}", url);
            }
            match download_once(url, dst).await {
                Ok(()) => {
                    if i > 0 {
                        println!("✓ 已从镜像下载");
                    }
                    return Ok(());
                }
                Err(e) => {
                    last_err = e;
                    if attempt < 2 {
                        tokio::time::sleep(std::time::Duration::from_millis(300 * attempt as u64))
                            .await;
                    }
                }
            }
        }
    }
    Err(last_err).with_context(|| {
        format!(
            "下载 {name} 失败（已尝试 {} 个源）。可设置 IAI_DOWNLOAD_MIRROR，或：\n  curl -fsSL https://iaiaiai.ai/install.sh | bash -s -- --version <tag>",
            candidates.len()
        )
    })
}

async fn download_once(url: &str, dst: &Path) -> Result<()> {
    let resp = client()
        .get(url)
        .send()
        .await
        .with_context(|| format!("GET {url} 失败"))?
        .error_for_status()
        .with_context(|| format!("下载 {url} 返回错误状态"))?;
    let bytes = resp.bytes().await.context("读取下载内容失败")?;
    if bytes.len() < 1_000 {
        bail!("下载内容过小（{} bytes），疑似错误页：{url}", bytes.len());
    }
    std::fs::write(dst, &bytes).with_context(|| format!("写入 {} 失败", dst.display()))?;
    Ok(())
}

async fn fetch_release(tag: Option<&str>) -> Result<ReleaseResp> {
    let url = match tag {
        Some(t) => format!("{API_TAG_FMT}{t}"),
        None => API_LATEST.to_string(),
    };
    let resp = client()
        .get(&url)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .with_context(|| format!("请求 {url} 失败（检查网络或仓库 {REPO_OWNER}/{REPO_NAME}）"))?
        .error_for_status()
        .with_context(|| format!("GitHub API 返回错误状态（tag={:?}）", tag))?;
    let r: ReleaseResp = resp
        .json()
        .await
        .context("解析 GitHub release JSON 失败")?;
    Ok(r)
}

fn find_asset<'a>(release: &'a ReleaseResp, target: &str) -> Option<&'a AssetResp> {
    let want = asset_name(&release.tag_name, target);
    release.assets.iter().find(|a| a.name == want)
}

pub struct UpgradeCheck {
    pub current: String,
    pub latest: String,
    pub latest_tag: String,
    pub published_at: Option<String>,
    pub has_update: bool,
    pub target: String,
    pub asset_name: String,
    pub has_sha256: bool,
    pub notes: Option<String>,
}

/// 仅检查，不下载。
pub async fn check() -> Result<UpgradeCheck> {
    let cur = current_version().to_string();
    let release = fetch_release(None).await?;
    let target = host_target_triple().to_string();
    let asset_name_str = asset_name(&release.tag_name, &target);
    let sha_name = format!("{}.sha256", asset_name_str);
    let latest = release.tag_name.trim_start_matches('v').to_string();
    let has_sha256 = release.assets.iter().any(|a| a.name == sha_name);
    Ok(UpgradeCheck {
        has_update: latest != cur,
        current: cur,
        latest,
        latest_tag: release.tag_name.clone(),
        published_at: release.published_at.clone(),
        target,
        asset_name: asset_name_str,
        has_sha256,
        notes: release.body,
    })
}

/// 检查 + 下载 + 安装 + 重启。
pub async fn run(target_version: Option<String>, yes: bool, no_restart: bool) -> Result<()> {
    let cur = current_version().to_string();
    let target = host_target_triple();
    println!("当前版本  v{cur}");
    println!("目标平台  {target}");

    // 1. 解析 release
    let release = fetch_release(target_version.as_deref())
        .await
        .context("无法获取 GitHub release 信息")?;
    let want_tag = release.tag_name.clone();
    let want_ver = want_tag.trim_start_matches('v').to_string();

    println!("目标版本  {want_tag}");
    if let Some(p) = &release.published_at {
        println!("发布时间  {p}");
    }

    if want_ver == cur {
        println!("✓ 已是最新版本");
        return Ok(());
    }

    // 2. 匹配平台资产
    let asset = find_asset(&release, target)
        .ok_or_else(|| {
            anyhow!(
                "release {want_tag} 不含本平台 {target} 的资产（应为 {}）。可用资产: {}",
                asset_name(&want_tag, target),
                release
                    .assets
                    .iter()
                    .map(|a| a.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })?
        .clone();
    let sha_name = format!("{}.sha256", asset.name);
    let sha_asset = release.assets.iter().find(|a| a.name == sha_name).cloned();

    // 3. 确认
    println!();
    println!("将执行：");
    println!("  ↓ 下载  {}", asset.name);
    if sha_asset.is_some() {
        println!("  ✓ SHA256 校验");
    } else {
        println!("  ⚠ 跳过 SHA256 校验（无 .sha256 文件）");
    }
    println!("  📦 备份 → 解压 → 替换 → 重启");
    println!();
    if !yes {
        print!("确认升级？[y/N] ");
        use std::io::Write;
        std::io::stdout().flush()?;
        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        if !matches!(input.trim().to_ascii_lowercase().as_str(), "y" | "yes") {
            println!("已取消");
            return Ok(());
        }
    }

    // 4. 临时目录
    let tmpdir = std::env::temp_dir().join(format!("iai-upgrade-{}", want_ver));
    let _ = std::fs::remove_dir_all(&tmpdir);
    std::fs::create_dir_all(&tmpdir)
        .with_context(|| format!("创建临时目录失败: {}", tmpdir.display()))?;
    let tar_path = tmpdir.join(&asset.name);

    // 5. 下载（直连失败则走官网镜像 / 公共加速）
    println!("↓ 下载 {}", asset.name);
    download_to_with_mirrors(&asset.browser_download_url, &tar_path, Some(&asset.name))
        .await
        .with_context(|| format!("下载 {} 失败", asset.name))?;

    // 6. SHA256 校验
    if let Some(sha_a) = sha_asset {
        let sha_path = tmpdir.join(&sha_a.name);
        println!("↓ 下载 {}", sha_a.name);
        download_to_with_mirrors(&sha_a.browser_download_url, &sha_path, Some(&sha_a.name))
            .await?;
        let expected = parse_sha256_file(&sha_path)?;
        let actual = sha256_file(&tar_path)?;
        if !expected.eq_ignore_ascii_case(&actual) {
            bail!("SHA256 校验失败：期望 {expected}，实际 {actual}");
        }
        println!("✓ SHA256 校验通过");
    } else {
        println!("⚠ release 未提供 .sha256 文件，仅依赖 TLS 完整性");
    }

    // 7. 解压（包布局为 `iai-vX-TARGET/iai`，兼容顶层直接放 `iai`）
    let extract_dir = tmpdir.join("extracted");
    std::fs::create_dir_all(&extract_dir)?;
    extract_tar_gz(&tar_path, &extract_dir).context("解压 tar.gz 失败")?;
    let new_bin = find_iai_binary(&extract_dir)
        .ok_or_else(|| anyhow!("解压后未找到 `iai` 二进制（已检查顶层与一级子目录）"))?;
    let meta = std::fs::metadata(&new_bin)?;
    if meta.len() < 100_000 {
        bail!("解压出的 `iai` 大小异常（{} bytes），疑似损坏", meta.len());
    }
    println!("✓ 解压得到 {}", new_bin.display());

    // 8. 备份 + 替换
    let self_path = std::env::current_exe().context("获取当前二进制路径失败")?;
    let ts = chrono::Utc::now().format("%Y%m%d-%H%M%S").to_string();
    let bak_path = append_suffix(&self_path, &format!("bak.{ts}"));
    println!(
        "⏳ 备份 {} → {}",
        self_path.display(),
        bak_path.display()
    );
    std::fs::copy(&self_path, &bak_path).context("备份旧二进制失败")?;

    let staging = append_suffix(&self_path, "new");
    std::fs::copy(&new_bin, &staging).context("复制新二进制到 staging 失败")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&staging, std::fs::Permissions::from_mode(0o755))?;
    }
    std::fs::rename(&staging, &self_path).context("原子替换失败")?;
    println!("✓ 已替换为 v{want_ver}");

    // 9. 自报版本
    match Command::new(&self_path).arg("--version").output() {
        Ok(out) if out.status.success() => {
            let v = String::from_utf8_lossy(&out.stdout);
            println!("✓ 新二进制自报: {}", v.trim());
        }
        Ok(out) => {
            let err = String::from_utf8_lossy(&out.stderr);
            bail!(
                "新二进制 --version 失败（exit={:?}）：\n{}\n可手动回滚：mv -f {} {}",
                out.status.code(),
                err.trim(),
                bak_path.display(),
                self_path.display()
            );
        }
        Err(e) => bail!(
            "新二进制无法执行：{e}\n可手动回滚：mv -f {} {}",
            bak_path.display(),
            self_path.display()
        ),
    }

    // 10. 重启 systemd
    if no_restart {
        println!("⚠ --no-restart 设置，请手动重启 iai 服务");
    } else if try_systemctl_restart("iai") {
        println!("✓ 已重启 systemd 服务 iai.service");
    } else {
        println!("⚠ 未找到 systemd 单元 iai.service，请手动重启");
    }

    // 11. 清理临时目录
    let _ = std::fs::remove_dir_all(&tmpdir);
    println!();
    println!("✅ 升级完成 v{cur} → v{want_ver}（旧版保留为 {}）", bak_path.display());
    Ok(())
}

fn parse_sha256_file(path: &Path) -> Result<String> {
    let body = std::fs::read_to_string(path)?;
    // 标准格式：`<hash>   <file>` 或 `<hash> *<file>`
    let hex = body
        .split_whitespace()
        .next()
        .ok_or_else(|| anyhow!("SHA256 文件内容为空"))?;
    if hex.len() != 64 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        bail!("SHA256 文件格式异常：{hex:?}");
    }
    Ok(hex.to_lowercase())
}

fn sha256_file(path: &Path) -> Result<String> {
    use sha2::{Digest, Sha256};
    use std::io::Read;
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 65536];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}

fn extract_tar_gz(archive: &Path, dst: &Path) -> Result<()> {
    let f = std::fs::File::open(archive)?;
    let gz = flate2::read::GzDecoder::new(f);
    let mut tar = tar::Archive::new(gz);
    // 解压到临时子目录，避免路径穿越
    tar.set_overwrite(true);
    tar.unpack(dst).context("解压 tar.gz 失败")?;
    Ok(())
}

/// 在解压目录中定位 `iai`：优先顶层，其次一级子目录（与 publish.sh 顶层目录布局对齐）。
fn find_iai_binary(extract_dir: &Path) -> Option<PathBuf> {
    let top = extract_dir.join("iai");
    if top.is_file() {
        return Some(top);
    }
    let Ok(entries) = std::fs::read_dir(extract_dir) else {
        return None;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let candidate = path.join("iai");
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    // 兜底：浅层递归找名为 iai 的文件（最多两层）
    for entry in std::fs::read_dir(extract_dir).ok()?.flatten() {
        let p = entry.path();
        if p.is_file() && p.file_name().and_then(|n| n.to_str()) == Some("iai") {
            return Some(p);
        }
        if p.is_dir() {
            for sub in std::fs::read_dir(&p).ok()?.flatten() {
                let sp = sub.path();
                if sp.is_file() && sp.file_name().and_then(|n| n.to_str()) == Some("iai") {
                    return Some(sp);
                }
            }
        }
    }
    None
}

fn try_systemctl_restart(unit: &str) -> bool {
    let cat = Command::new("systemctl").args(["cat", unit]).output();
    let Ok(cat) = cat else {
        return false;
    };
    if !cat.status.success() {
        return false;
    }
    Command::new("systemctl")
        .args(["restart", unit])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// 给路径追加后缀：`/usr/local/bin/iai` + `.bak.20260619-153000` → `/usr/local/bin/iai.bak.20260619-153000`
fn append_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut s = path.as_os_str().to_owned();
    s.push(".");
    s.push(suffix);
    PathBuf::from(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("iai-upgrade-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn find_iai_in_nested_release_layout() {
        let dir = scratch_dir("nested");
        let nested = dir.join("iai-v0.4.3-macos-aarch64");
        std::fs::create_dir_all(&nested).unwrap();
        let bin = nested.join("iai");
        std::fs::write(&bin, vec![0u8; 120_000]).unwrap();
        let found = find_iai_binary(&dir).unwrap();
        assert_eq!(found, bin);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn find_iai_at_archive_root() {
        let dir = scratch_dir("root");
        let bin = dir.join("iai");
        std::fs::write(&bin, vec![0u8; 120_000]).unwrap();
        let found = find_iai_binary(&dir).unwrap();
        assert_eq!(found, bin);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
