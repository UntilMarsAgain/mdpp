use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use crate::project::{Project, config, under};
use crate::server::http;

/// 要盯着的那几条路径
pub(super) fn watched(project: &Project) -> Vec<PathBuf> {
    let mut paths = vec![
        project.pages_dir(),
        project.templates_dir(),
        project.root.join(config::FILE_NAME),
    ];
    for file in project
        .config
        .extra_css
        .iter()
        .chain(&project.config.extra_js)
        .chain(&project.config.favicon)
    {
        paths.push(under(&project.root, file));
    }
    paths
}

/// 盯着几条路径：手上一张「文件 → 大小与改动时间」的照，和刚扫的那张比
pub(super) struct Watch {
    paths: Vec<PathBuf>,
    seen: BTreeMap<PathBuf, (u64, SystemTime)>,
}

impl Watch {
    pub(super) fn new(paths: Vec<PathBuf>) -> Self {
        let seen = http::collect(&paths);
        Watch { paths, seen }
    }

    /// 等一次改动（没等到就等满 `limit`）。
    ///
    /// 这一层**只看不动**那张照 —— 动了的话 `diff` 就说不出「改了什么」。
    /// 分片睡而不是一次睡到底：`Ctrl-C` 是默认的退出方式，而睡死了别的线程也喊不动它
    pub(super) fn wait(&self, limit: Duration) {
        let step = Duration::from_millis(50);
        let mut waited = Duration::ZERO;
        while waited < limit {
            std::thread::sleep(step.min(limit - waited));
            waited += step;
            if http::collect(&self.paths) != self.seen {
                return;
            }
        }
    }

    /// 等它**稳下来**：从现在起连着 `limit` 毫秒没有新变化，才返回。
    ///
    /// 与 [`Watch::wait`] 比的不是同一张照：`wait` 问的是「与上次收下的那份还一样吗」，
    /// 而改动已经发生了，回不去 —— 拿它当判据会变成「看见改动立刻返回」，防抖就没了。
    /// 这里比的是**上一眼**：连着几眼都一样，才算写完了。
    ///
    /// 也不能一直等下去：真有人（或某个进程）一直在写，得有个头 —— 等满
    /// `limit × 4` 还不稳就先出，下一轮眼睛还盯着。返回值说这一次是不是真等稳了。
    ///
    /// **只看不动 `seen`**：照还得留给 [`Watch::diff`] 去收，它要拿旧照比出「改了哪些」
    pub(super) fn settle(&self, limit: Duration) -> bool {
        let step = Duration::from_millis(50);
        let cap = limit.saturating_mul(4);
        let mut latest = http::collect(&self.paths);
        let mut quiet = Duration::ZERO;
        let mut waited = Duration::ZERO;

        while waited < cap {
            let nap = step.min(limit - quiet);
            std::thread::sleep(nap);
            waited += nap;

            let now = http::collect(&self.paths);
            if now == latest {
                quiet += nap;
                if quiet >= limit {
                    return true;
                }
            } else {
                latest = now;
                quiet = Duration::ZERO;
            }
        }
        // 等到超时还没稳：不赖在这儿了，先出，下一轮再看
        false
    }

    /// 扫一遍，把这一轮照收下，返回改了 / 增了 / 删了的那几个
    ///
    /// **删掉的也要算一次改动**：少了一页是改动，而只比「现在有的」是发现不了它的
    pub(super) fn diff(&mut self) -> Option<Vec<PathBuf>> {
        let now = http::collect(&self.paths);
        let mut changed: Vec<PathBuf> = now
            .iter()
            .filter(|(path, mark)| self.seen.get(*path) != Some(*mark))
            .map(|(path, _)| path.clone())
            .collect();
        changed.extend(
            self.seen
                .keys()
                .filter(|path| !now.contains_key(*path))
                .cloned(),
        );
        changed.sort();
        self.seen = now;
        (!changed.is_empty()).then_some(changed)
    }
}
