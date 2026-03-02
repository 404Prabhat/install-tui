use crate::model::PackageRecord;

use super::{App, SearchJob};

const MAX_MATCHES: usize = 500;
const SEARCH_BATCH_SIZE: usize = 1200;

impl App {
    pub(super) fn refresh_matches(&mut self) {
        self.search_job = None;
        self.search_loading = false;

        if self.packages.is_empty() {
            self.matches.clear();
            self.result_cursor = 0;
            return;
        }

        if self.search_query.trim().is_empty() {
            self.matches = (0..self.packages.len().min(MAX_MATCHES)).collect();
            self.result_cursor = 0;
            return;
        }

        self.matches.clear();
        self.result_cursor = 0;
        self.search_loading = true;
        self.search_job = Some(SearchJob {
            query: self.search_query.to_ascii_lowercase(),
            cursor: 0,
            scored: Vec::new(),
        });
    }

    pub(super) fn advance_search_job(&mut self) {
        let Some(mut job) = self.search_job.take() else {
            return;
        };

        let start = job.cursor;
        let end = (start + SEARCH_BATCH_SIZE).min(self.packages.len());

        for idx in start..end {
            if let Some(score) = fuzzy_score(&self.packages[idx], &job.query) {
                job.scored.push((score, idx));
            }
        }

        if end < self.packages.len() {
            job.cursor = end;
            self.search_job = Some(job);
            self.search_loading = true;
            return;
        }

        job.scored.sort_unstable_by(|left, right| {
            right
                .0
                .cmp(&left.0)
                .then_with(|| self.packages[left.1].name.cmp(&self.packages[right.1].name))
        });

        self.matches = job
            .scored
            .into_iter()
            .take(MAX_MATCHES)
            .map(|(_, index)| index)
            .collect();
        self.search_loading = false;
    }
}

fn fuzzy_score(pkg: &PackageRecord, query: &str) -> Option<i64> {
    if query.is_empty() {
        return Some(0);
    }

    let mut score = 0i64;
    let mut q_iter = query.chars();
    let mut current = q_iter.next()?;
    let mut consecutive = 0i64;

    for (idx, ch) in pkg.lower.chars().enumerate() {
        if ch == current {
            consecutive += 1;
            score += 10 + consecutive * 6;
            score -= idx as i64;

            if let Some(next) = q_iter.next() {
                current = next;
            } else {
                score += 100;
                return Some(score);
            }
        } else {
            consecutive = 0;
        }
    }

    None
}

pub(super) fn parse_packages(input: &str) -> Vec<String> {
    input
        .split(|ch: char| ch == ',' || ch.is_whitespace())
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(|part| {
            part.chars()
                .filter(|ch| {
                    ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | '+' | '@')
                })
                .collect::<String>()
        })
        .filter(|pkg| !pkg.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::parse_packages;

    #[test]
    fn parse_packages_handles_commas_spaces_and_filters_invalid_chars() {
        let parsed = parse_packages(" fastfetch,btop   nvtop@1   !!bad!! ");
        assert_eq!(parsed, vec!["fastfetch", "btop", "nvtop@1", "bad"]);
    }
}
