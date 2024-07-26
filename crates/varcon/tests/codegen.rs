#![allow(clippy::self_named_module_files)] // false positive

const DICT: &[u8] = include_bytes!("../assets/varcon.txt");

#[test]
fn codegen() {
    let mut content = vec![];
    generate(&mut content);

    let content = String::from_utf8(content).unwrap();
    let content = codegenrs::rustfmt(&content, None).unwrap();
    snapbox::assert_data_eq!(content, snapbox::file!["../src/codegen.rs"].raw());
}

fn generate<W: std::io::Write>(file: &mut W) {
    let dict = String::from_utf8_lossy(DICT);
    let clusters = varcon_core::ClusterIter::new(&dict);

    writeln!(
        file,
        "// This file is @generated by {}",
        file!().replace('\\', "/")
    )
    .unwrap();
    writeln!(file, "#![allow(clippy::unreadable_literal)]",).unwrap();
    writeln!(file).unwrap();
    writeln!(
        file,
        "use crate::{{Category, Cluster, Entry, Pos, Tag, Type, Variant}};"
    )
    .unwrap();
    writeln!(file).unwrap();

    writeln!(file, "pub static VARCON: &[Cluster] = &[").unwrap();
    for mut cluster in clusters {
        cluster.infer();
        writeln!(file, "Cluster {{").unwrap();
        writeln!(file, "  header: {:?},", cluster.header).unwrap();
        writeln!(file, "  entries: &[").unwrap();
        for entry in &cluster.entries {
            writeln!(file, "  Entry {{").unwrap();
            writeln!(file, "    variants: &[").unwrap();
            for variant in &entry.variants {
                writeln!(file, "      Variant {{").unwrap();
                writeln!(file, "        word: {:?},", variant.word).unwrap();
                writeln!(file, "        types: &[").unwrap();
                for t in &variant.types {
                    write!(file, "          Type {{").unwrap();
                    write!(file, "category: Category::{:?}, ", t.category).unwrap();
                    if let Some(tag) = t.tag {
                        write!(file, "tag: Some(Tag::{tag:?}), ").unwrap();
                    } else {
                        write!(file, "tag: {:?}, ", t.tag).unwrap();
                    }
                    write!(file, "num: {:?},", t.num).unwrap();
                    writeln!(file, "}},").unwrap();
                }
                writeln!(file, "        ],").unwrap();
                writeln!(file, "      }},").unwrap();
            }
            writeln!(file, "  ],").unwrap();
            if let Some(pos) = entry.pos {
                write!(file, "  pos: Some(Pos::{pos:?}),").unwrap();
            } else {
                write!(file, "  pos: {:?},", entry.pos).unwrap();
            }
            writeln!(
                file,
                " archaic: {:?}, note: {:?},",
                entry.archaic, entry.note
            )
            .unwrap();
            writeln!(file, "  description: {:?},", entry.description).unwrap();
            writeln!(file, "  comment: {:?},", entry.comment).unwrap();
            writeln!(file, "  }},").unwrap();
        }
        writeln!(file, "  ],").unwrap();
        writeln!(file, "  notes: &[").unwrap();
        for note in &cluster.notes {
            writeln!(file, "    {note:?},").unwrap();
        }
        writeln!(file, "  ],").unwrap();
        writeln!(file, "  }},").unwrap();
    }
    writeln!(file, "];").unwrap();
}
