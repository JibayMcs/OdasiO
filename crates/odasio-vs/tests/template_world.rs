//! Validation sur un monde généré par le jeu (`test-worlds/template-flat.vcdbs`, non versionné).

use std::path::Path;

use odasio_vs::proto::ProtoReader;
use odasio_vs::{VsDatabase, layer};

#[test]
fn decodes_a_game_written_world() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test-worlds/template-flat.vcdbs");
    if !path.is_file() {
        eprintln!("monde modèle absent, test ignoré");
        return;
    }
    let copy = std::env::temp_dir().join("odasio-template-test.vcdbs");
    std::fs::copy(&path, &copy).unwrap();
    let db = VsDatabase::open(&copy).unwrap();
    assert_eq!(db.save.map_size[1], 256);
    let ids = db.save.block_ids().unwrap();
    assert_eq!(ids["game:air"], 0);
    let granite = ids["game:rock-granite"];
    assert!(granite > 0);

    // Chunk du bas d'une colonne superflat, écrit par le jeu : roche + sol.
    let conn = rusqlite::Connection::open(&copy).unwrap();
    let blob: Vec<u8> = conn
        .query_row(
            "SELECT data FROM chunk WHERE position = ?1",
            [odasio_vs::position_key(16000, 0, 16000)],
            |r| r.get(0),
        )
        .unwrap();
    let field = ProtoReader::new(&blob)
        .map(Result::unwrap)
        .find(|f| f.number == 1)
        .unwrap();
    let cells = layer::decode(field.value.as_bytes().unwrap()).unwrap();
    let by_id: std::collections::HashMap<u32, &String> =
        ids.iter().map(|(code, id)| (*id, code)).collect();
    let names: std::collections::BTreeSet<_> = cells.iter().map(|id| by_id[id]).collect();
    assert!(
        names.iter().any(|n| n.starts_with("game:soil-")),
        "{names:?}"
    );
    assert!(
        names.iter().any(|n| n.starts_with("game:rock-")),
        "{names:?}"
    );
}
