use asterism_proto::types::{DiffSide, ReviewComment, ReviewThread};

use crate::store::StoredReviewComment;

pub struct FilePatch<'a> {
    pub path: String,
    pub text: &'a str,
}

pub fn split_patch(patch: &str) -> Vec<FilePatch<'_>> {
    let mut starts: Vec<usize> = patch
        .match_indices("diff --git ")
        .filter(|(i, _)| *i == 0 || patch.as_bytes()[i - 1] == b'\n')
        .map(|(i, _)| i)
        .collect();
    starts.push(patch.len());
    starts
        .windows(2)
        .map(|w| {
            let text = &patch[w[0]..w[1]];
            let header = |prefix: &str| {
                text.lines()
                    .find_map(|l| l.strip_prefix(prefix))
                    .filter(|p| *p != "/dev/null")
                    .map(|p| p.split_once('/').map_or(p, |(_, rest)| rest).to_string())
            };
            let path = header("+++ ")
                .or_else(|| header("--- "))
                .unwrap_or_default();
            FilePatch { path, text }
        })
        .collect()
}

pub fn line_text(patch: &str, path: &str, side: DiffSide, line: u32) -> Option<String> {
    let file = split_patch(patch).into_iter().find(|f| f.path == path)?;
    let (mut old, mut new) = (0u32, 0u32);
    let mut in_hunk = false;
    for l in file.text.lines() {
        if let Some(rest) = l.strip_prefix("@@ -") {
            let (o, n) = rest.split_once(" +")?;
            old = o.split(',').next()?.parse().ok()?;
            new = n.split([',', ' ']).next()?.parse().ok()?;
            in_hunk = true;
            continue;
        }
        if !in_hunk || l.starts_with('\\') {
            continue;
        }
        let (tag, body) = l.split_at(l.len().min(1));
        let hit = match (tag, side) {
            ("-", DiffSide::Old) | (" ", DiffSide::Old) => old == line,
            ("+", DiffSide::New) | (" ", DiffSide::New) => new == line,
            _ => false,
        };
        if hit {
            return Some(body.to_string());
        }
        match tag {
            "-" => old += 1,
            "+" => new += 1,
            _ => {
                old += 1;
                new += 1;
            }
        }
    }
    None
}

/// FNV-1a, stable across Rust versions so stored hashes stay valid.
pub fn hash(text: &str) -> String {
    let h = text.bytes().fold(0xcbf29ce484222325u64, |h, b| {
        (h ^ b as u64).wrapping_mul(0x100000001b3)
    });
    format!("{h:016x}")
}

pub fn local_threads(comments: &[StoredReviewComment], patch: &str) -> Vec<ReviewThread> {
    let mut threads: Vec<ReviewThread> = Vec::new();
    for c in comments.iter().filter(|c| !c.published) {
        let comment = ReviewComment {
            id: c.id.to_string(),
            author: "local".into(),
            body: c.body.clone(),
            created_at: c.created_at.to_string(),
        };
        match threads.iter_mut().find(|t| t.id == c.thread_id) {
            Some(t) => t.comments.push(comment),
            None => threads.push(ReviewThread {
                id: c.thread_id.clone(),
                path: c.path.clone(),
                line: c.line,
                side: c.side,
                outdated: line_text(patch, &c.path, c.side, c.line).as_deref()
                    != Some(c.line_text.as_str()),
                resolved: c.resolved,
                local: true,
                pending: false,
                comments: vec![comment],
            }),
        }
    }
    threads
}

pub fn prompt(threads: &[&ReviewThread], patch: &str) -> String {
    let mut out = String::from("Review comments on your changes:\n");
    for t in threads {
        let side = if t.side == DiffSide::Old {
            " (old)"
        } else {
            ""
        };
        out.push_str(&format!("\n{}:{}{side}\n", t.path, t.line));
        if let Some(code) = line_text(patch, &t.path, t.side, t.line) {
            out.push_str(&format!("> {code}\n"));
        }
        for c in &t.comments {
            let who = if t.local {
                "local".to_string()
            } else {
                format!("@{}", c.author)
            };
            out.push_str(&format!("- ({who}) {}\n", c.body));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const PATCH: &str = "diff --git a/a.rs b/a.rs
index 1..2 100644
--- a/a.rs
+++ b/a.rs
@@ -1,3 +1,3 @@
 fn main() {
-    old();
+    new();
 }
diff --git a/gone.rs b/gone.rs
deleted file mode 100644
--- a/gone.rs
+++ /dev/null
@@ -1 +0,0 @@
-bye
";

    fn stored(
        id: i64,
        thread: &str,
        line: u32,
        side: DiffSide,
        text: &str,
        body: &str,
    ) -> StoredReviewComment {
        StoredReviewComment {
            id,
            thread_id: thread.into(),
            path: "a.rs".into(),
            line,
            side,
            line_text: text.into(),
            body: body.into(),
            resolved: false,
            published: false,
            created_at: 1,
        }
    }

    #[test]
    fn patches_split_per_file_and_deleted_files_keep_their_old_path() {
        let files = split_patch(PATCH);
        assert_eq!(
            files.iter().map(|f| f.path.as_str()).collect::<Vec<_>>(),
            vec!["a.rs", "gone.rs"]
        );
        assert!(files[0].text.starts_with("diff --git a/a.rs") && files[0].text.ends_with(" }\n"));
    }

    #[test]
    fn lines_are_found_on_either_side() {
        assert_eq!(
            line_text(PATCH, "a.rs", DiffSide::New, 2).as_deref(),
            Some("    new();")
        );
        assert_eq!(
            line_text(PATCH, "a.rs", DiffSide::Old, 2).as_deref(),
            Some("    old();")
        );
        assert_eq!(
            line_text(PATCH, "a.rs", DiffSide::New, 3).as_deref(),
            Some("}")
        );
        assert_eq!(
            line_text(PATCH, "gone.rs", DiffSide::Old, 1).as_deref(),
            Some("bye")
        );
        assert_eq!(line_text(PATCH, "a.rs", DiffSide::New, 9), None);
    }

    #[test]
    fn edited_lines_make_local_threads_outdated() {
        let comments = vec![
            stored(1, "local-1", 2, DiffSide::New, "    new();", "ok"),
            stored(2, "local-1", 2, DiffSide::New, "    new();", "reply"),
            stored(3, "local-3", 2, DiffSide::New, "    newer();", "stale"),
        ];
        let threads = local_threads(&comments, PATCH);
        assert_eq!(threads.len(), 2);
        assert!(threads[0].local && !threads[0].outdated && threads[0].comments.len() == 2);
        assert!(threads[1].outdated);
    }

    #[test]
    fn published_local_threads_are_hidden() {
        let mut c = stored(1, "local-1", 2, DiffSide::New, "    new();", "ok");
        c.published = true;
        assert!(local_threads(&[c], PATCH).is_empty());
    }

    #[test]
    fn hashes_are_stable() {
        assert_eq!(hash("abc"), hash("abc"));
        assert_ne!(hash("abc"), hash("abd"));
    }

    #[test]
    fn prompts_quote_the_line_and_attribute_comments() {
        let mut t = local_threads(
            &[stored(
                1,
                "local-1",
                2,
                DiffSide::New,
                "    new();",
                "Add a test.",
            )],
            PATCH,
        );
        t.push(ReviewThread {
            id: "T1".into(),
            path: "a.rs".into(),
            line: 2,
            side: DiffSide::Old,
            outdated: false,
            resolved: false,
            local: false,
            pending: false,
            comments: vec![ReviewComment {
                id: "C".into(),
                author: "alice".into(),
                body: "Why?".into(),
                created_at: "".into(),
            }],
        });
        let refs: Vec<&ReviewThread> = t.iter().collect();
        assert_eq!(
            prompt(&refs, PATCH),
            "Review comments on your changes:\n\na.rs:2\n>     new();\n- (local) Add a test.\n\na.rs:2 (old)\n>     old();\n- (@alice) Why?\n"
        );
    }
}
