//! PDF documents built in code for the PDF tests (spec 027 part C), so the
//! fixtures say what they contain instead of being opaque binaries.

use lopdf::content::{Content, Operation};
use lopdf::{
    Dictionary, Document, EncryptionState, EncryptionVersion, Object, Permissions, Stream, dictionary,
};

/// What to put in a fixture PDF.
#[derive(Debug, Clone, Default)]
pub struct PdfSpec {
    pub title: Option<String>,
    pub author: Option<String>,
    pub subject: Option<String>,
    pub keywords: Option<String>,
    pub created: Option<String>,
    pub lang: Option<String>,
    pub tagged: bool,
    /// The text of each page; an empty string is a page with no text.
    pub pages: Vec<String>,
    /// `(page, uri)`, 1-based pages.
    pub links: Vec<(u32, String)>,
    /// Pages drawn as an image only, like a scan.
    pub scanned: bool,
    /// Protect with this user password.
    pub password: Option<String>,
    /// Extra bytes in an unused stream, to reach a size.
    pub padding: usize,
}

impl PdfSpec {
    pub fn titled(title: &str, pages: usize) -> Self {
        Self {
            title: Some(title.into()),
            pages: (1..=pages)
                .map(|n| format!("Page {n} of the report with some words"))
                .collect(),
            ..Self::default()
        }
    }

    pub fn build(&self) -> Vec<u8> {
        let mut doc = Document::with_version("1.5");
        let pages_id = doc.new_object_id();
        let font = doc.add_object(dictionary! {
            "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica",
        });
        let image = doc.add_object(Stream::new(
            dictionary! {
                "Type" => "XObject", "Subtype" => "Image", "Width" => 2, "Height" => 2,
                "ColorSpace" => "DeviceGray", "BitsPerComponent" => 8,
            },
            vec![0, 255, 255, 0],
        ));
        let resources = doc.add_object(dictionary! {
            "Font" => dictionary! { "F1" => font },
            "XObject" => dictionary! { "Im1" => image },
        });
        let mut kids = Vec::new();
        for (i, text) in self.pages.iter().enumerate() {
            let n = i as u32 + 1;
            let ops = if self.scanned {
                vec![
                    Operation::new("q", vec![]),
                    Operation::new(
                        "cm",
                        vec![500.into(), 0.into(), 0.into(), 700.into(), 50.into(), 50.into()],
                    ),
                    Operation::new("Do", vec!["Im1".into()]),
                    Operation::new("Q", vec![]),
                ]
            } else if text.is_empty() {
                vec![]
            } else {
                vec![
                    Operation::new("BT", vec![]),
                    Operation::new("Tf", vec!["F1".into(), 12.into()]),
                    Operation::new("Td", vec![72.into(), 720.into()]),
                    Operation::new("Tj", vec![Object::string_literal(text.as_str())]),
                    Operation::new("ET", vec![]),
                ]
            };
            let content = doc.add_object(Stream::new(
                dictionary! {},
                Content { operations: ops }.encode().expect("content encodes"),
            ));
            let annots: Vec<Object> = self
                .links
                .iter()
                .filter(|(p, _)| *p == n)
                .map(|(_, uri)| {
                    doc.add_object(dictionary! {
                        "Type" => "Annot", "Subtype" => "Link",
                        "Rect" => vec![72.into(), 700.into(), 300.into(), 714.into()],
                        "A" => dictionary! { "S" => "URI", "URI" => Object::string_literal(uri.as_str()) },
                    })
                    .into()
                })
                .collect();
            let mut page = dictionary! {
                "Type" => "Page", "Parent" => pages_id, "Contents" => content,
                "Resources" => resources, "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            };
            if !annots.is_empty() {
                page.set("Annots", annots);
            }
            kids.push(doc.add_object(page).into());
        }
        let count = kids.len() as i64;
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! { "Type" => "Pages", "Kids" => kids, "Count" => count }),
        );
        let mut catalog = dictionary! { "Type" => "Catalog", "Pages" => pages_id };
        if let Some(l) = &self.lang {
            catalog.set("Lang", Object::string_literal(l.as_str()));
        }
        if self.tagged {
            catalog.set("MarkInfo", dictionary! { "Marked" => true });
        }
        let catalog = doc.add_object(catalog);
        doc.trailer.set("Root", catalog);
        let mut info = Dictionary::new();
        for (k, v) in [
            ("Title", &self.title),
            ("Author", &self.author),
            ("Subject", &self.subject),
            ("Keywords", &self.keywords),
            ("CreationDate", &self.created),
        ] {
            if let Some(v) = v {
                info.set(k, Object::string_literal(v.as_str()));
            }
        }
        let info = doc.add_object(info);
        doc.trailer.set("Info", info);
        if self.padding > 0 {
            doc.add_object(Stream::new(dictionary! {}, vec![b'x'; self.padding]));
        }
        if let Some(password) = &self.password {
            doc.trailer.set(
                "ID",
                vec![
                    Object::string_literal("0123456789abcdef"),
                    Object::string_literal("0123456789abcdef"),
                ],
            );
            let state = EncryptionState::try_from(EncryptionVersion::V2 {
                document: &doc,
                owner_password: "owner",
                user_password: password,
                key_length: 128,
                permissions: Permissions::all(),
            })
            .expect("encryption state");
            doc.encrypt(&state).expect("encrypts");
        }
        let mut out = Vec::new();
        doc.save_to(&mut out).expect("saves");
        out
    }
}
