//! 候选窗口的底色：系统材质（毛玻璃）/ 不透明系统色 / 半透明色，对应配置项 `candidate_background`。

use serde::{Deserialize, Serialize};

/// 候选窗口底下垫什么。
///
/// 系统材质（毛玻璃）最好看，但窗口服务器不一定给窗口配 backdrop —— macOS 上候选窗这块置顶面板
/// 实测一直拿不到（见 `apps/macos/src/ui/material.rs`），所以留两档不依赖材质的：`Solid` 干净，
/// `Translucent` 透着看但不模糊。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CandidateBackground {
    /// 系统材质（毛玻璃 / Liquid Glass），垫在候选内容最底下。
    #[default]
    Material,

    /// 不透明的系统窗口色，像系统自带的候选框。
    Solid,

    /// 半透明的系统窗口色：看得见背后（不模糊），比材质闷一点。
    Translucent,
}

impl CandidateBackground {
    /// 全部取值，设置界面按这个顺序列出。
    pub const ALL: [Self; 3] = [Self::Material, Self::Solid, Self::Translucent];

    /// 配置文件里的写法。
    pub fn key(self) -> &'static str {
        match self {
            Self::Material => "material",
            Self::Solid => "solid",
            Self::Translucent => "translucent",
        }
    }

    /// 界面上的名字。
    pub fn label(self) -> &'static str {
        match self {
            Self::Material => "系统材质（毛玻璃）",
            Self::Solid => "不透明",
            Self::Translucent => "半透明",
        }
    }

    /// 候选自己那层底色的不透明度：材质那档要留出材质，另外两档直接当纯色底用。
    pub fn alpha(self) -> f64 {
        match self {
            Self::Material => 0.3,
            Self::Solid => 1.0,
            Self::Translucent => 0.72,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_round_trip_and_parse() {
        // 与配置文件里同一种形式（`[general] candidate_background = "…"`）
        #[derive(Deserialize)]
        struct Probe {
            candidate_background: CandidateBackground,
        }
        for value in CandidateBackground::ALL {
            let text = format!("candidate_background = \"{}\"\n", value.key());
            let parsed: Probe = toml::from_str(&text).unwrap();
            assert_eq!(parsed.candidate_background, value);
        }
        assert_eq!(
            CandidateBackground::default(),
            CandidateBackground::Material
        );
        assert!(CandidateBackground::ALL.iter().all(|v| v.alpha() > 0.0));
    }
}
