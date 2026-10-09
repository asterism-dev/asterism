use asterism_proto::types::{DiffSide, ReviewComment, ReviewThread};

use crate::store::StoredReviewComment;

pub struct FilePatch<'a> {
    pub path: String,
    pub text: &'a str,
}

fn unquote(raw: &str) -> String {
    let raw = raw.trim_end_matches('\t');
    let Some(inner) = raw.strip_prefix('"').and_then(|r| r.strip_suffix('"')) else {
        return raw.to_string();
    };
    let (mut out, mut b) = (Vec::new(), inner.bytes().peekable());
    while let Some(c) = b.next() {
        if c != b'\\' {
            out.push(c);
            continue;
        }
        match b.next() {
            Some(d @ b'0'..=b'7') => {
                let mut n = d - b'0';
                for _ in 0..2 {
                    if let Some(o) = b.next_if(|o| (b'0'..=b'7').contains(o)) {
                        n = n.wrapping_mul(8).wrapping_add(o - b'0');
                    }
                }
                out.push(n);
            }
            Some(b't') => out.push(b'\t'),
            Some(b'n') => out.push(b'\n'),
            Some(o) => out.push(o),
            None => {}
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn patch_path(text: &str) -> String {
    let header: Vec<&str> = text.lines().take_while(|l| !l.starts_with("@@")).collect();
    let side = |prefix: &str| {
        header
            .iter()
            .find_map(|l| l.strip_prefix(prefix))
            .map(unquote)
            .filter(|p| p != "/dev/null")
            .map(|p| {
                p.split_once('/')
                    .map_or(p.clone(), |(_, rest)| rest.to_string())
            })
    };
    side("+++ ")
        .or_else(|| side("--- "))
        .or_else(|| {
            header
                .iter()
                .find_map(|l| l.strip_prefix("rename to "))
                .map(unquote)
        })
        .or_else(|| {
            let line = header.first()?.strip_prefix("diff --git ")?;
            let at = if line.ends_with('"') {
                line.rfind("\"b/")
            } else {
                line.rfind(" b/").map(|i| i + 1)
            }?;
            Some(unquote(&line[at..]).split_once('/')?.1.to_string())
        })
        .unwrap_or_default()
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
            FilePatch {
                path: patch_path(text),
                text,
            }
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
        let Some(tag @ ('-' | '+' | ' ')) = l.chars().next() else {
            continue;
        };
        let body = &l[1..];
        let hit = match (tag, side) {
            ('-' | ' ', DiffSide::Old) => old == line,
            ('+' | ' ', DiffSide::New) => new == line,
            _ => false,
        };
        if hit {
            return Some(body.to_string());
        }
        match tag {
            '-' => old += 1,
            '+' => new += 1,
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

    const GIT: &str = "diff --git \"a/caf\\303\\251.rs\" \"b/caf\\303\\251.rs\"
index 587be6b..975fbec 100644
--- \"a/caf\\303\\251.rs\"
+++ \"b/caf\\303\\251.rs\"
@@ -1 +1 @@
-x
+y
diff --git a/my file.rs b/my file.rs
index b3addbe..7e43fc4 100644
--- a/my file.rs\t
+++ b/my file.rs\t
@@ -1,2 +1,2 @@
 one
-last
\\ No newline at end of file
+last2
\\ No newline at end of file
diff --git a/old.rs b/new.rs
similarity index 100%
rename from old.rs
rename to new.rs
diff --git a/u.rs b/u.rs
index 66b4ba3..f971f2b 100644
--- a/u.rs
+++ b/u.rs
@@ -1 +1,2 @@
 \u{e9} line
+z
";

    #[test]
    fn real_git_output_paths_and_lines() {
        let paths: Vec<String> = split_patch(GIT).into_iter().map(|f| f.path).collect();
        assert_eq!(paths, vec!["caf\u{e9}.rs", "my file.rs", "new.rs", "u.rs"]);
        assert_eq!(
            line_text(GIT, "caf\u{e9}.rs", DiffSide::New, 1).as_deref(),
            Some("y")
        );
        assert_eq!(
            line_text(GIT, "my file.rs", DiffSide::New, 2).as_deref(),
            Some("last2")
        );
        assert_eq!(
            line_text(GIT, "my file.rs", DiffSide::Old, 2).as_deref(),
            Some("last")
        );
        assert_eq!(line_text(GIT, "my file.rs", DiffSide::New, 3), None);
        assert_eq!(
            line_text(GIT, "u.rs", DiffSide::New, 1).as_deref(),
            Some("\u{e9} line")
        );
        assert_eq!(
            line_text(GIT, "u.rs", DiffSide::New, 2).as_deref(),
            Some("z")
        );
    }

    #[test]
    fn multibyte_garbage_lines_do_not_panic() {
        let p = "diff --git a/a b/a\n--- a/a\n+++ b/a\n@@ -1 +1 @@\n\u{e9}x\n+ok\n";
        assert_eq!(line_text(p, "a", DiffSide::New, 1).as_deref(), Some("ok"));
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
