use objc2_foundation::{NSBundle, NSString};

/// 从主 bundle 的 Info.plist 读出的输入法身份信息。
#[derive(Debug, Clone)]
pub struct BundleInfo {
    /// `InputMethodConnectionName`，IMK 用它注册 NSConnection。
    pub connection_name: String,

    /// `CFBundleIdentifier`。
    pub identifier: String,

    /// `CFBundleShortVersionString`，菜单里显示。
    pub version: String,

    /// 构建标识：打包脚本通过环境变量 `QINGJIAN_BUILD` 塞进来的 git 短哈希与日期；直接 `cargo build` 的是「本地构建」。
    pub build: String,
}

impl BundleInfo {
    /// 读不到时回退到编译期常量，方便在 `.app` 之外直接跑二进制看日志。
    pub fn from_main_bundle() -> Self {
        let bundle = NSBundle::mainBundle();
        let identifier = bundle
            .bundleIdentifier()
            .map(|s| s.to_string())
            .unwrap_or_else(|| DEFAULT_IDENTIFIER.to_owned());
        let connection_name = bundle
            .objectForInfoDictionaryKey(&NSString::from_str("InputMethodConnectionName"))
            .and_then(|value| value.downcast::<NSString>().ok())
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("{identifier}_Connection"));
        let version = bundle
            .objectForInfoDictionaryKey(&NSString::from_str("CFBundleShortVersionString"))
            .and_then(|value| value.downcast::<NSString>().ok())
            .map(|s| s.to_string())
            .unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_owned());
        Self {
            connection_name,
            identifier,
            version: version.clone(),
            build: build_summary(
                &version,
                option_env!("QINGJIAN_BUILD").unwrap_or("本地构建"),
            ),
        }
    }
}

/// 构建串：`bundle.sh` 给的是 `${git 短哈希} · ${日期}`，而**开发版的版本号里已经带了同一个哈希**
/// （`0.1.5-dev-5b145d3+`）——两处都显示就是重复，这里把构建串里那段哈希去掉、只留日期。
/// 正式版版本号不带哈希（`0.1.3`），构建串原样保留：诊断时还得靠它认是哪个提交。
fn build_summary(version: &str, build: &str) -> String {
    match build.split_once(" · ") {
        Some((hash, rest)) if version.ends_with(hash) => rest.to_owned(),
        _ => build.to_owned(),
    }
}

/// 与 `Info.plist` 里的 `CFBundleIdentifier` 保持一致。
pub(crate) const DEFAULT_IDENTIFIER: &str = "app.qingjian.inputmethod";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_summary_drops_a_hash_already_in_the_version() {
        // 开发版：版本号里已经有短哈希了，构建串只留日期
        assert_eq!(
            build_summary("0.1.5-dev-5b145d3+", "5b145d3+ · 2026-10-07"),
            "2026-10-07"
        );
        assert_eq!(
            build_summary("0.1.5-dev-5b145d3", "5b145d3 · 2026-10-07"),
            "2026-10-07"
        );
        // 正式版：版本号里没有哈希，构建串原样
        assert_eq!(
            build_summary("0.1.3", "5b145d3 · 2026-10-07"),
            "5b145d3 · 2026-10-07"
        );
        // 直接 cargo build：没有构建串，也没有日期可拆
        assert_eq!(build_summary("0.1.5-dev", "本地构建"), "本地构建");
    }
}
