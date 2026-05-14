//! Pure parser for CourtListener bulk dumps.
//!
//! A bulk dump is a `.tar.gz` of one JSON file per opinion. Malformed JSON
//! is reported as `Err` in the result vec — never aborts the whole parse.
//!
//! Sprint-2 implementation collects results into a `Vec` because the tar
//! crate's iterator-of-Entry borrows the archive and is awkward to wrap in
//! a self-referential `Iterator`. For the volumes in scope (≤ ~10k opinions
//! per court / ~100 MB of opinion text in RAM) this is acceptable. Real
//! streaming is a Sprint-3 follow-up if scale demands it.

use std::io::Read;

use chrono::NaiveDate;
use serde::Deserialize;

/// Parsed CourtListener opinion ready for upsert into `case_documents`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Opinion {
    pub opinion_id: i64,
    pub court_id: String,
    pub case_name: Option<String>,
    pub date_filed: Option<NaiveDate>,
    pub citation_count: i32,
    pub full_text_plain: String,
    pub source_url: Option<String>,
    /// S6.5 — `opinions_cited` array from CourtListener (REST path), or
    /// `None` when the source did not carry citation data (bulk-tarball
    /// path).  `Some(vec![])` means "fetched and no citations".
    pub cites: Option<Vec<String>>,
}

/// Raw on-disk shape (CourtListener bulk format).
#[derive(Deserialize)]
struct RawOpinion {
    id: i64,
    court: String,
    case_name: Option<String>,
    date_filed: Option<String>,
    #[serde(default)]
    citation_count: i32,
    plain_text: String,
    absolute_url: Option<String>,
}

impl RawOpinion {
    fn into_opinion(self) -> Opinion {
        let date_filed = self
            .date_filed
            .as_deref()
            .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok());
        Opinion {
            opinion_id: self.id,
            court_id: self.court,
            case_name: self.case_name,
            date_filed,
            citation_count: self.citation_count,
            full_text_plain: self.plain_text,
            source_url: self.absolute_url,
            // S6.5 — bulk-tarball dumps don't carry cites; the REST path
            // does.  None here means "never fetched", which keeps the
            // back-fill worker's "WHERE cites_extracted_at IS NULL" scan
            // honest.
            cites: None,
        }
    }
}

/// Parse a `.tar.gz` reader into a `Vec<Result<Opinion>>`.
///
/// `Err` results are *not* fatal — they represent malformed entries that
/// should be logged and skipped. Callers iterate the vec and decide what
/// to do per entry.
pub fn parse_tarball<R: Read>(reader: R) -> Vec<anyhow::Result<Opinion>> {
    let gz = flate2::read::GzDecoder::new(reader);
    let mut archive = tar::Archive::new(gz);

    let entries = match archive.entries() {
        Ok(it) => it,
        Err(e) => return vec![Err(anyhow::anyhow!("tar entries iterator: {e}"))],
    };

    let mut out = Vec::new();
    for entry_result in entries {
        let mut entry = match entry_result {
            Ok(e) => e,
            Err(e) => {
                out.push(Err(anyhow::anyhow!("tar entry: {e}")));
                continue;
            }
        };

        if entry.header().entry_type().is_dir() {
            continue;
        }

        let mut buf = String::new();
        if let Err(e) = entry.read_to_string(&mut buf) {
            out.push(Err(anyhow::anyhow!("read tar entry: {e}")));
            continue;
        }

        match serde_json::from_str::<RawOpinion>(&buf) {
            Ok(raw) => out.push(Ok(raw.into_opinion())),
            Err(e) => {
                let snippet: String = buf.chars().take(80).collect();
                out.push(Err(anyhow::anyhow!(
                    "malformed opinion JSON: {e} (snippet: {snippet})"
                )));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_tarball(entries: &[(&str, &str)]) -> Vec<u8> {
        let mut gz_buf = Vec::new();
        {
            let gz = flate2::write::GzEncoder::new(&mut gz_buf, flate2::Compression::default());
            let mut tar_w = tar::Builder::new(gz);
            for (name, body) in entries {
                let bytes = body.as_bytes();
                let mut header = tar::Header::new_gnu();
                header.set_size(bytes.len() as u64);
                header.set_mode(0o644);
                header.set_cksum();
                tar_w
                    .append_data(&mut header, name, bytes)
                    .expect("tar append");
            }
            tar_w.finish().expect("tar finish");
        }
        gz_buf
    }

    fn good_opinion(id: i64) -> String {
        format!(
            r#"{{"id":{id},"court":"tax","case_name":"In re Test {id}","date_filed":"2024-06-15","citation_count":3,"plain_text":"This is the body of opinion {id}.","absolute_url":"https://courtlistener.com/opinion/{id}/"}}"#
        )
    }

    #[test]
    fn parses_well_formed_tarball() {
        let tarball = build_tarball(&[
            ("op1.json", &good_opinion(101)),
            ("op2.json", &good_opinion(102)),
            ("op3.json", &good_opinion(103)),
        ]);
        let results = parse_tarball(&tarball[..]);
        assert_eq!(results.len(), 3);
        let opinions: Vec<Opinion> = results
            .into_iter()
            .map(|r| r.expect("all entries valid"))
            .collect();
        assert_eq!(opinions[0].opinion_id, 101);
        assert_eq!(opinions[0].court_id, "tax");
        assert_eq!(opinions[0].case_name.as_deref(), Some("In re Test 101"));
        assert_eq!(
            opinions[0].date_filed,
            Some(NaiveDate::from_ymd_opt(2024, 6, 15).unwrap())
        );
        assert_eq!(opinions[0].citation_count, 3);
        assert!(opinions[0].full_text_plain.contains("opinion 101"));
    }

    #[test]
    fn malformed_entry_yields_err_and_does_not_abort() {
        let tarball = build_tarball(&[
            ("ok.json", &good_opinion(201)),
            ("bad.json", "{not json"),
            ("ok2.json", &good_opinion(202)),
        ]);
        let results = parse_tarball(&tarball[..]);
        assert_eq!(results.len(), 3, "all three entries surfaced");
        assert!(results[0].is_ok());
        assert!(results[1].is_err(), "bad entry surfaced as Err");
        assert!(results[2].is_ok(), "iteration continued past the bad entry");
    }

    #[test]
    fn missing_optional_fields_default_sensibly() {
        let json = r#"{"id":301,"court":"tax","plain_text":"minimal opinion body"}"#;
        let tarball = build_tarball(&[("min.json", json)]);
        let results = parse_tarball(&tarball[..]);
        let opinion = results.into_iter().next().unwrap().expect("parses");
        assert_eq!(opinion.opinion_id, 301);
        assert_eq!(opinion.case_name, None);
        assert_eq!(opinion.date_filed, None);
        assert_eq!(opinion.source_url, None);
        assert_eq!(opinion.citation_count, 0, "default applied");
    }
}
