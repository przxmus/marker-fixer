use quick_xml::Reader;
use quick_xml::escape::escape;
use quick_xml::events::Event;
use uuid::Uuid;

use crate::error::{MarkerFixerError, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Marker {
    pub start_frame: u64,
    pub name: Option<String>,
    pub comment: Option<String>,
    pub guid: String,
}

#[derive(Debug, Clone)]
pub struct ParsedMarkers {
    pub frame_rate: Option<String>,
    pub markers: Vec<Marker>,
}

pub fn chapter_start_to_frame(start_seconds: f64, fps: f64) -> u64 {
    (start_seconds * fps).floor().max(0.0) as u64
}

pub fn marker_from_chapter(start_seconds: f64, title: Option<&str>, fps: f64) -> Marker {
    Marker {
        start_frame: chapter_start_to_frame(start_seconds, fps),
        name: title
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        comment: None,
        guid: Uuid::new_v4().to_string(),
    }
}

pub fn parse_markers(xmp_xml: &str) -> Result<ParsedMarkers> {
    if !xmp_xml.contains("<x:xmpmeta") || !xmp_xml.contains("</x:xmpmeta>") {
        return Err(MarkerFixerError::InvalidXmp(
            "XMP payload is missing required <x:xmpmeta> boundaries".to_string(),
        ));
    }

    let mut reader = Reader::from_str(xmp_xml);
    reader.config_mut().trim_text(true);

    let mut in_markers_track = false;
    let mut in_markers_seq = false;
    let mut frame_rate: Option<String> = None;
    let mut markers = Vec::new();

    loop {
        match reader.read_event() {
            Ok(Event::Start(ref e)) => {
                let name = e.name();
                if name.as_ref() == b"rdf:Description" {
                    let mut track_name = None;
                    let mut track_frame_rate = None;
                    let mut start_time = None;
                    let mut marker_name = None;
                    let mut marker_comment = None;
                    let mut marker_guid = None;

                    for attr in e.attributes().flatten() {
                        let key = attr.key.as_ref();
                        let value =
                            attr.decode_and_unescape_value(reader.decoder())
                                .map_err(|err| {
                                    MarkerFixerError::InvalidXmp(format!(
                                        "failed to decode xmp attribute: {err}"
                                    ))
                                })?;

                        match key {
                            b"xmpDM:trackName" => track_name = Some(value.to_string()),
                            b"xmpDM:frameRate" => track_frame_rate = Some(value.to_string()),
                            b"xmpDM:startTime" => start_time = Some(value.to_string()),
                            b"xmpDM:name" => marker_name = Some(value.to_string()),
                            b"xmpDM:comment" => marker_comment = Some(value.to_string()),
                            b"xmpDM:guid" => marker_guid = Some(value.to_string()),
                            _ => {}
                        }
                    }

                    if track_name.as_deref() == Some("Markers") {
                        in_markers_track = true;
                        if let Some(rate) = track_frame_rate {
                            frame_rate = Some(rate);
                        }
                    }

                    if in_markers_seq {
                        if let Some(start_time) = start_time {
                            let start_frame = start_time.parse::<u64>().map_err(|err| {
                                MarkerFixerError::InvalidXmp(format!(
                                    "invalid marker startTime '{start_time}': {err}"
                                ))
                            })?;
                            markers.push(Marker {
                                start_frame,
                                name: marker_name.filter(|value| !value.trim().is_empty()),
                                comment: marker_comment.filter(|value| !value.trim().is_empty()),
                                guid: marker_guid.unwrap_or_else(|| Uuid::new_v4().to_string()),
                            });
                        }
                    }
                } else if name.as_ref() == b"xmpDM:markers" && in_markers_track {
                    in_markers_seq = true;
                }
            }
            Ok(Event::End(ref e)) => {
                let name = e.name();
                if name.as_ref() == b"xmpDM:markers" {
                    in_markers_seq = false;
                } else if name.as_ref() == b"rdf:Description" && in_markers_track && !in_markers_seq
                {
                    in_markers_track = false;
                }
            }
            Ok(Event::Eof) => break,
            Err(err) => {
                return Err(MarkerFixerError::InvalidXmp(format!(
                    "invalid xmp xml at position {}: {err}",
                    reader.buffer_position()
                )));
            }
            _ => {}
        }
    }

    Ok(ParsedMarkers {
        frame_rate,
        markers,
    })
}

pub fn merge_markers(existing: Vec<Marker>, incoming: Vec<Marker>) -> Vec<Marker> {
    use std::collections::BTreeMap;

    let mut by_start = BTreeMap::<u64, Marker>::new();
    for marker in existing.into_iter().chain(incoming) {
        by_start
            .entry(marker.start_frame)
            .and_modify(|current| {
                if marker_has_text(&marker) && !marker_has_text(current) {
                    *current = marker.clone();
                }
            })
            .or_insert(marker);
    }

    by_start.into_values().collect()
}

fn marker_has_text(marker: &Marker) -> bool {
    marker
        .name
        .as_deref()
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false)
        || marker
            .comment
            .as_deref()
            .map(|value| !value.trim().is_empty())
            .unwrap_or(false)
}

pub fn generate_xmp(frame_rate: &str, markers: &[Marker]) -> String {
    let mut xml = String::new();
    xml.push_str("<?xpacket begin=\"\u{feff}\" id=\"W5M0MpCehiHzreSzNTczkc9d\"?>\n");
    xml.push_str("<x:xmpmeta xmlns:x=\"adobe:ns:meta/\">\n");
    xml.push_str(" <rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\">\n");
    xml.push_str("  <rdf:Description rdf:about=\"\" xmlns:xmpDM=\"http://ns.adobe.com/xmp/1.0/DynamicMedia/\">\n");
    xml.push_str("   <xmpDM:Tracks>\n");
    xml.push_str("    <rdf:Bag>\n");
    xml.push_str("     <rdf:li>\n");
    xml.push_str("      <rdf:Description xmpDM:trackName=\"Markers\" xmpDM:frameRate=\"");
    xml.push_str(&escape(frame_rate));
    xml.push_str("\">\n");
    xml.push_str("       <xmpDM:markers>\n");
    xml.push_str("        <rdf:Seq>\n");

    for marker in markers {
        xml.push_str("         <rdf:li>\n");
        xml.push_str("          <rdf:Description xmpDM:startTime=\"");
        xml.push_str(&marker.start_frame.to_string());
        xml.push_str("\" xmpDM:type=\"Comment\" xmpDM:guid=\"");
        xml.push_str(&escape(&marker.guid));
        xml.push('\"');

        if let Some(name) = marker
            .name
            .as_deref()
            .filter(|value| !value.trim().is_empty())
        {
            xml.push_str(" xmpDM:name=\"");
            xml.push_str(&escape(name));
            xml.push('\"');
        }

        if let Some(comment) = marker
            .comment
            .as_deref()
            .filter(|value| !value.trim().is_empty())
        {
            xml.push_str(" xmpDM:comment=\"");
            xml.push_str(&escape(comment));
            xml.push('\"');
        }

        xml.push_str("/>\n");
        xml.push_str("         </rdf:li>\n");
    }

    xml.push_str("        </rdf:Seq>\n");
    xml.push_str("       </xmpDM:markers>\n");
    xml.push_str("      </rdf:Description>\n");
    xml.push_str("     </rdf:li>\n");
    xml.push_str("    </rdf:Bag>\n");
    xml.push_str("   </xmpDM:Tracks>\n");
    xml.push_str("  </rdf:Description>\n");
    xml.push_str(" </rdf:RDF>\n");
    xml.push_str("</x:xmpmeta>\n");
    xml.push_str("<?xpacket end=\"w\"?>\n");
    xml
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_calculation_uses_floor() {
        assert_eq!(chapter_start_to_frame(0.8, 60.0), 48);
        assert_eq!(chapter_start_to_frame(2.35, 60.0), 141);
    }

    #[test]
    fn merge_prefers_marker_with_text_for_same_frame() {
        let existing = vec![Marker {
            start_frame: 100,
            name: None,
            comment: None,
            guid: "a".to_string(),
        }];
        let incoming = vec![Marker {
            start_frame: 100,
            name: Some("Text".to_string()),
            comment: None,
            guid: "b".to_string(),
        }];

        let merged = merge_markers(existing, incoming);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].name.as_deref(), Some("Text"));
        assert_eq!(merged[0].guid, "b");
    }

    #[test]
    fn generated_xml_contains_marker_track() {
        let xml = generate_xmp(
            "f60",
            &[Marker {
                start_frame: 10,
                name: Some("GOAT".to_string()),
                comment: None,
                guid: "123".to_string(),
            }],
        );

        assert!(xml.contains("xmpDM:trackName=\"Markers\""));
        assert!(xml.contains("xmpDM:startTime=\"10\""));
        assert!(xml.contains("xmpDM:name=\"GOAT\""));
    }
}
