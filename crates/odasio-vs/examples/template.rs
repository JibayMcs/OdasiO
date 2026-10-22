//! Génère un monde modèle avec des réglages de monde, pour vérifier qu'ils sont appliqués.
//! Usage : cargo run -p odasio-vs --example template -- <dossier> [code=valeur ...]

use std::time::Duration;

use odasio_vs::{TemplateOptions, VsInstall, template};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let work = std::path::PathBuf::from(args.next().ok_or("dossier de travail attendu")?);
    let mut opts = TemplateOptions {
        world_type: "standard".into(),
        ..Default::default()
    };
    for setting in args {
        let (code, value) = setting.split_once('=').ok_or("code=valeur attendu")?;
        opts.world_config.insert(code.into(), value.into());
    }
    let install = VsInstall::detect().ok_or("Vintage Story introuvable")?;
    let save = template::generate(&install, &work, &opts, Duration::from_secs(300))?;
    println!("{}", save.display());
    Ok(())
}
