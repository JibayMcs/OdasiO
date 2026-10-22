//! Génération d'un monde modèle par le serveur dédié Vintage Story, sans interface.
//!
//! Le monde modèle fournit un `gamedata` cohérent (table des IDs de blocs, configuration)
//! dans lequel on injecte ensuite les colonnes converties.

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use crate::{Result, VsError};

static CONFIG_TEMPLATE: &str = include_str!("../../../tools/vs/template-serverconfig.json");
const READY_MARKER: &str = "Dedicated Server now running";

#[derive(Debug, Clone)]
pub struct TemplateOptions {
    pub world_name: String,
    /// Hauteur du monde, multiple de 32.
    pub map_size_y: u32,
    pub seed: String,
    /// `superflat` : la passe Végétation ne fait que l'éclairage, ce qui permet au jeu
    /// d'éclairer nos colonnes.
    pub world_type: String,
    pub play_style: String,
}

impl Default for TemplateOptions {
    fn default() -> Self {
        Self {
            world_name: "Odasio".into(),
            map_size_y: 384,
            seed: "12345".into(),
            world_type: "superflat".into(),
            play_style: "creativebuilding".into(),
        }
    }
}

/// Installation de Vintage Story : binaire du serveur et runtime .NET à utiliser.
#[derive(Debug, Clone)]
pub struct VsInstall {
    pub server: PathBuf,
    /// `DOTNET_ROOT` à transmettre quand le runtime est installé hors du système.
    pub dotnet_root: Option<PathBuf>,
}

impl VsInstall {
    pub fn detect() -> Option<Self> {
        let home = std::env::var_os("HOME").map(PathBuf::from);
        let appdata = std::env::var_os("APPDATA").map(PathBuf::from);
        let candidates = [
            home.as_ref()
                .map(|h| h.join(".local/share/vintagestory/VintagestoryServer")),
            appdata
                .as_ref()
                .map(|a| a.join("Vintagestory/VintagestoryServer.exe")),
        ];
        let server = candidates.into_iter().flatten().find(|p| p.is_file())?;
        let dotnet_root = home
            .map(|h| h.join(".dotnet"))
            .filter(|d| d.join("shared/Microsoft.NETCore.App").is_dir());
        Some(Self {
            server,
            dotnet_root,
        })
    }
}

fn free_port() -> Result<u16> {
    Ok(TcpListener::bind("127.0.0.1:0")?.local_addr()?.port())
}

fn server_config(data_dir: &Path, save: &Path, opts: &TemplateOptions) -> Result<String> {
    let mut config: serde_json::Value = serde_json::from_str(CONFIG_TEMPLATE)?;
    config["Port"] = free_port()?.into();
    config["MapSizeY"] = opts.map_size_y.into();
    config["ModPaths"] = serde_json::json!(["Mods", data_dir.join("Mods")]);
    let world = &mut config["WorldConfig"];
    world["Seed"] = opts.seed.clone().into();
    world["SaveFileLocation"] = save.to_string_lossy().into_owned().into();
    world["WorldName"] = opts.world_name.clone().into();
    world["WorldType"] = opts.world_type.clone().into();
    world["PlayStyle"] = opts.play_style.clone().into();
    world["PlayStyleLangCode"] = opts.play_style.clone().into();
    Ok(serde_json::to_string_pretty(&config)?)
}

/// Lance le serveur sur un dossier de données jetable, attend qu'il ait créé le monde,
/// l'arrête proprement et renvoie le chemin du `.vcdbs` produit.
pub fn generate(
    install: &VsInstall,
    work_dir: &Path,
    opts: &TemplateOptions,
    timeout: Duration,
) -> Result<PathBuf> {
    let data_dir = work_dir.join("vsdata");
    std::fs::create_dir_all(data_dir.join("Mods"))?;
    let save = data_dir.join("Saves").join("template.vcdbs");
    if save.exists() {
        std::fs::remove_file(&save)?;
    }
    std::fs::write(
        data_dir.join("serverconfig.json"),
        server_config(&data_dir, &save, opts)?,
    )?;

    let mut command = Command::new(&install.server);
    command
        .arg("--dataPath")
        .arg(&data_dir)
        .current_dir(install.server.parent().unwrap_or(Path::new(".")))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    if let Some(root) = &install.dotnet_root {
        command.env("DOTNET_ROOT", root);
    }
    let mut child = command.spawn()?;
    let stdout = child.stdout.take().expect("stdout redirigé");

    let (ready_tx, ready_rx) = mpsc::channel();
    let reader = std::thread::spawn(move || {
        let mut log = Vec::new();
        for line in BufReader::new(stdout)
            .lines()
            .map_while(std::io::Result::ok)
        {
            if line.contains(READY_MARKER) {
                let _ = ready_tx.send(());
            }
            log.push(line);
        }
        log
    });

    let ready = ready_rx.recv_timeout(timeout).is_ok();
    if ready && let Some(stdin) = child.stdin.as_mut() {
        writeln!(stdin, "/stop")?;
        stdin.flush()?;
    } else {
        let _ = child.kill();
    }
    let status = child.wait()?;
    let log = reader.join().unwrap_or_default();

    if !ready || !save.is_file() {
        let tail = log[log.len().saturating_sub(15)..].join("\n");
        return Err(VsError::Server(format!(
            "le serveur n'a pas créé le monde modèle ({status})\n{tail}"
        )));
    }
    Ok(save)
}
