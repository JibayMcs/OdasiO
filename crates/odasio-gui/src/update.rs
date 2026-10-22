//! Mises à jour automatiques depuis les releases GitHub du projet.
//!
//! Au démarrage (au plus une fois par jour), la dernière release publique est comparée à la
//! version en cours. La mise à jour s'installe selon la façon dont OdasiO a été installé :
//! installateur Windows relancé en mode silencieux, paquet `.deb` installé avec `pkexec`, ou
//! exécutable portable remplacé sur place. Chaque fichier téléchargé est vérifié avec la
//! liste `SHA256SUMS` publiée avec la release.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, anyhow, bail};
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::recent;

/// Dépôt GitHub des releases.
pub const REPO: &str = "JibayMcs/OdasiO";
/// Délai minimal entre deux vérifications automatiques.
const CHECK_EVERY: Duration = Duration::from_secs(24 * 3600);
const USER_AGENT: &str = concat!("OdasiO/", env!("CARGO_PKG_VERSION"));

pub fn current_version() -> semver::Version {
    semver::Version::parse(env!("CARGO_PKG_VERSION")).expect("version Cargo valide")
}

#[derive(Debug, Clone)]
pub struct Release {
    pub version: semver::Version,
    pub notes: String,
    /// Page de la release sur GitHub.
    pub page: String,
    pub assets: Vec<Asset>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Asset {
    pub name: String,
    #[serde(rename = "browser_download_url")]
    pub url: String,
    #[serde(default)]
    pub size: u64,
}

/// Façon dont OdasiO est installé, qui décide de la manière de le mettre à jour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Install {
    /// Installateur Windows (`OdasiO-x.y.z-windows-x64-setup.exe`).
    WindowsInstaller,
    /// Paquet Debian (`odasio_x.y.z_amd64.deb`), exécutable dans `/usr`.
    Deb,
    /// Archive Linux (`odasio-x.y.z-linux-x86_64.tar.gz`), exécutable remplacé sur place.
    Portable,
}

impl Install {
    pub fn detect() -> Install {
        if cfg!(windows) {
            return Install::WindowsInstaller;
        }
        match std::env::current_exe() {
            Ok(exe) if exe.starts_with("/usr") => Install::Deb,
            _ => Install::Portable,
        }
    }

    /// Le fichier de la release qui convient à cette installation.
    pub fn asset(self, release: &Release) -> Option<&Asset> {
        let suffix = match self {
            Install::WindowsInstaller => "-windows-x64-setup.exe",
            Install::Deb => "_amd64.deb",
            Install::Portable => "-linux-x86_64.tar.gz",
        };
        release.assets.iter().find(|a| a.name.ends_with(suffix))
    }
}

/// Résultat d'une installation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Applied {
    /// Nouvelle version en place : il suffit de relancer.
    Restart,
    /// L'installateur a pris le relais : OdasiO doit se fermer.
    Exit,
    /// Le paquet a été ouvert dans le gestionnaire de logiciels du système.
    Manual,
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(10))
        .timeout_read(Duration::from_secs(60))
        .user_agent(USER_AGENT)
        .build()
}

/// Désactivé par `ODASIO_NO_UPDATE`, et dans les compilations de développement.
pub fn enabled() -> bool {
    std::env::var_os("ODASIO_NO_UPDATE").is_none() && !cfg!(debug_assertions)
}

/// Vrai si la dernière vérification automatique date de plus d'un jour ; la note.
pub fn due() -> bool {
    let file = recent::config_file("update-check.txt");
    let last = file
        .as_ref()
        .and_then(|f| std::fs::read_to_string(f).ok())
        .and_then(|t| t.trim().parse::<u64>().ok())
        .unwrap_or(0);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    if now.saturating_sub(last) < CHECK_EVERY.as_secs() {
        return false;
    }
    if let Some(f) = file {
        let _ = std::fs::create_dir_all(f.parent().unwrap_or(Path::new(".")));
        let _ = std::fs::write(f, now.to_string());
    }
    true
}

/// Dernière release publiée, si elle est plus récente que la version en cours.
/// `ODASIO_UPDATE_URL` remplace l'adresse de l'API (tests).
pub fn check() -> Result<Option<Release>> {
    #[derive(Deserialize)]
    struct Raw {
        tag_name: String,
        #[serde(default)]
        body: String,
        html_url: String,
        #[serde(default)]
        assets: Vec<Asset>,
        #[serde(default)]
        draft: bool,
        #[serde(default)]
        prerelease: bool,
    }
    let url = std::env::var("ODASIO_UPDATE_URL")
        .unwrap_or_else(|_| format!("https://api.github.com/repos/{REPO}/releases/latest"));
    let response = match agent()
        .get(&url)
        .set("Accept", "application/vnd.github+json")
        .call()
    {
        Ok(r) => r,
        // Aucune release publiée (ou dépôt privé) : rien à proposer.
        Err(ureq::Error::Status(404, _)) => return Ok(None),
        Err(e) => return Err(e).context("vérification des mises à jour"),
    };
    let raw: Raw = serde_json::from_reader(response.into_reader())?;
    if raw.draft || raw.prerelease {
        return Ok(None);
    }
    let version = semver::Version::parse(raw.tag_name.trim_start_matches('v'))
        .with_context(|| format!("version de release illisible : {}", raw.tag_name))?;
    Ok((version > current_version()).then_some(Release {
        version,
        notes: raw.body,
        page: raw.html_url,
        assets: raw.assets,
    }))
}

/// Télécharge le fichier de la release dans un dossier temporaire et vérifie son empreinte.
/// `progress` reçoit l'avancement (0..1).
pub fn download(release: &Release, asset: &Asset, progress: &dyn Fn(f32)) -> Result<PathBuf> {
    let sums = release
        .assets
        .iter()
        .find(|a| a.name == "SHA256SUMS")
        .ok_or_else(|| anyhow!("la release ne publie pas de SHA256SUMS"))?;
    let mut text = String::new();
    agent()
        .get(&sums.url)
        .call()?
        .into_reader()
        .read_to_string(&mut text)?;
    let expected = expected_sum(&text, &asset.name)
        .ok_or_else(|| anyhow!("{} absent de SHA256SUMS", asset.name))?;

    let dir = std::env::temp_dir().join("odasio-update");
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(&asset.name);
    let response = agent().get(&asset.url).call()?;
    let total = response
        .header("Content-Length")
        .and_then(|l| l.parse::<u64>().ok())
        .unwrap_or(asset.size)
        .max(1);
    let mut reader = response.into_reader();
    let mut file = std::fs::File::create(&path)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 64 * 1024];
    let mut done = 0u64;
    loop {
        let n = reader.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
        std::io::Write::write_all(&mut file, &buffer[..n])?;
        done += n as u64;
        progress((done as f32 / total as f32).min(1.0));
    }
    let actual: String = hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    if actual != expected {
        let _ = std::fs::remove_file(&path);
        bail!("empreinte SHA-256 incorrecte pour {}", asset.name);
    }
    Ok(path)
}

/// Empreinte attendue d'un fichier dans une liste au format `sha256sum`.
fn expected_sum(sums: &str, name: &str) -> Option<String> {
    sums.lines().find_map(|line| {
        let (hash, file) = line.split_once(char::is_whitespace)?;
        let file = file.trim().trim_start_matches('*');
        (file == name).then(|| hash.to_ascii_lowercase())
    })
}

/// Installe le fichier téléchargé.
pub fn apply(install: Install, file: &Path) -> Result<Applied> {
    match install {
        Install::WindowsInstaller => {
            // L'installateur ferme OdasiO, remplace les fichiers et le relance.
            Command::new(file)
                .args(["/SILENT", "/CLOSEAPPLICATIONS", "/RESTARTAPPLICATIONS"])
                .spawn()
                .context("lancement de l'installateur")?;
            Ok(Applied::Exit)
        }
        Install::Deb => {
            let status = Command::new("pkexec")
                .args(["apt-get", "install", "-y", "--allow-downgrades"])
                .arg(file)
                .status();
            match status {
                Ok(s) if s.success() => Ok(Applied::Restart),
                // pkexec absent ou authentification refusée : le gestionnaire de logiciels
                // du système prend le relais.
                _ => {
                    Command::new("xdg-open")
                        .arg(file)
                        .spawn()
                        .context("ouverture du paquet")?;
                    Ok(Applied::Manual)
                }
            }
        }
        Install::Portable => {
            let exe = std::env::current_exe()?;
            let dir = file.with_extension("").with_extension("");
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir)?;
            tar::Archive::new(flate2::read::GzDecoder::new(std::fs::File::open(file)?))
                .unpack(&dir)?;
            let find = |name: &str| -> Option<PathBuf> {
                walk(&dir)
                    .into_iter()
                    .find(|p| p.file_name().is_some_and(|n| n == name))
            };
            let gui =
                find("odasio-gui").ok_or_else(|| anyhow!("odasio-gui absent de l'archive"))?;
            self_replace::self_replace(&gui).context("remplacement de l'exécutable")?;
            // La ligne de commande, si elle est installée à côté.
            if let (Some(cli), Some(parent)) = (find("odasio"), exe.parent()) {
                let target = parent.join("odasio");
                if target.is_file() {
                    let staged = parent.join(".odasio.new");
                    if std::fs::copy(&cli, &staged).is_ok() {
                        let _ = std::fs::rename(&staged, &target);
                    }
                }
            }
            Ok(Applied::Restart)
        }
    }
}

fn walk(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for entry in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                out.push(path);
            }
        }
    }
    out
}

/// Relance OdasiO (nouvelle version) et quitte celui-ci.
pub fn restart() -> ! {
    if let Ok(exe) = std::env::current_exe() {
        let _ = Command::new(exe).spawn();
    }
    std::process::exit(0);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release(names: &[&str]) -> Release {
        Release {
            version: semver::Version::new(9, 0, 0),
            notes: String::new(),
            page: String::new(),
            assets: names
                .iter()
                .map(|n| Asset {
                    name: n.to_string(),
                    url: String::new(),
                    size: 0,
                })
                .collect(),
        }
    }

    #[test]
    fn each_install_finds_its_file() {
        let r = release(&[
            "SHA256SUMS",
            "odasio_9.0.0_amd64.deb",
            "odasio-9.0.0-linux-x86_64.tar.gz",
            "OdasiO-9.0.0-windows-x64-setup.exe",
        ]);
        assert_eq!(
            Install::Deb.asset(&r).unwrap().name,
            "odasio_9.0.0_amd64.deb"
        );
        assert_eq!(
            Install::Portable.asset(&r).unwrap().name,
            "odasio-9.0.0-linux-x86_64.tar.gz"
        );
        assert_eq!(
            Install::WindowsInstaller.asset(&r).unwrap().name,
            "OdasiO-9.0.0-windows-x64-setup.exe"
        );
    }

    #[test]
    fn reads_sha256sums() {
        let sums = "ABCDEF  odasio_9.0.0_amd64.deb\n0123 *OdasiO-9.0.0-windows-x64-setup.exe\n";
        assert_eq!(
            expected_sum(sums, "odasio_9.0.0_amd64.deb").as_deref(),
            Some("abcdef")
        );
        assert_eq!(
            expected_sum(sums, "OdasiO-9.0.0-windows-x64-setup.exe").as_deref(),
            Some("0123")
        );
        assert_eq!(expected_sum(sums, "autre"), None);
    }
}

/// État des mises à jour dans l'interface.
#[derive(Default)]
pub struct Updater {
    check: Option<crate::task::Task<Option<Release>>>,
    /// Vérification demandée par l'utilisateur : on dit aussi quand tout est à jour.
    manual: bool,
    dismissed: bool,
    install: Option<crate::task::Task<Applied>>,
    /// Instant (secondes egui) jusqu'auquel « à jour » reste affiché.
    up_to_date_until: Option<f64>,
}

impl Updater {
    /// Vérification automatique au démarrage.
    pub fn start_auto(&mut self, ctx: &eframe::egui::Context) {
        if enabled() && due() {
            self.start(ctx, false);
        }
    }

    pub fn start(&mut self, ctx: &eframe::egui::Context, manual: bool) {
        if self
            .check
            .as_ref()
            .is_some_and(crate::task::Task::is_running)
        {
            return;
        }
        self.manual = manual;
        self.dismissed = false;
        self.check = Some(crate::task::Task::spawn(
            ctx,
            crate::i18n::tr("update.checking"),
            |_| check(),
        ));
    }

    fn release(&self) -> Option<Release> {
        self.check.as_ref().and_then(|t| t.take_done()).flatten()
    }

    /// Numéro de version dans l'en-tête : un clic vérifie les mises à jour.
    pub fn version_button(&mut self, ui: &mut eframe::egui::Ui) {
        use crate::i18n::tr;
        use eframe::egui::RichText;
        let p = crate::theme::Palette::of(ui);
        let now = ui.input(|i| i.time);
        // Fin d'une vérification manuelle sans nouvelle version.
        if self.manual
            && let Some(task) = &self.check
            && task.with(|s| matches!(s, crate::task::State::Done(None)))
        {
            self.manual = false;
            self.up_to_date_until = Some(now + 4.0);
        }
        if let Some(until) = self.up_to_date_until {
            if now < until {
                ui.label(
                    RichText::new(tr("update.up_to_date"))
                        .size(12.5)
                        .color(p.success),
                );
                ui.ctx().request_repaint_after(Duration::from_millis(500));
                return;
            }
            self.up_to_date_until = None;
        }
        let checking = self
            .check
            .as_ref()
            .is_some_and(crate::task::Task::is_running);
        let text = RichText::new(format!("v{}", env!("CARGO_PKG_VERSION")))
            .size(12.5)
            .color(p.muted);
        let response = ui
            .add_enabled(!checking, eframe::egui::Button::new(text).frame(false))
            .on_hover_text(tr("update.check"));
        if response.clicked() {
            self.start(ui.ctx(), true);
        }
    }

    /// Bandeau sous l'en-tête : nouvelle version, installation, résultat.
    pub fn banner(&mut self, ctx: &eframe::egui::Context) {
        use crate::i18n::{tr, trf};
        use crate::task::{State, Task};
        use eframe::egui::{self, RichText};

        let failed_check = self.manual
            && self
                .check
                .as_ref()
                .and_then(|t| t.with(|s| matches!(s, State::Failed(_)).then_some(())))
                .is_some();
        let release = self.release();
        let installing = self.install.is_some();
        if (release.is_none() || self.dismissed) && !installing && !failed_check {
            return;
        }
        let p = crate::theme::Palette::of_ctx(ctx);
        egui::TopBottomPanel::top("update")
            .frame(
                egui::Frame::new()
                    .fill(p.accent.gamma_multiply(0.16))
                    .inner_margin(egui::Margin::symmetric(24, 10)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if failed_check {
                        let error = self
                            .check
                            .as_ref()
                            .and_then(|t| {
                                t.with(|s| match s {
                                    State::Failed(e) => Some(e.clone()),
                                    _ => None,
                                })
                            })
                            .unwrap_or_default();
                        ui.label(
                            RichText::new(trf("update.check_failed", &[("error", &error)]))
                                .color(p.danger),
                        );
                        if crate::theme::secondary_button(ui, tr("update.later")).clicked() {
                            self.manual = false;
                            self.check = None;
                        }
                        return;
                    }
                    if let Some(task) = &self.install {
                        let outcome = task.with(|s| match s {
                            State::Running { progress, message } => {
                                Err((*progress, message.clone()))
                            }
                            State::Done(applied) => Ok(Ok(applied.clone())),
                            State::Failed(e) => Ok(Err(e.clone())),
                        });
                        match outcome {
                            Err((progress, message)) => {
                                ui.label(message);
                                ui.add(
                                    egui::ProgressBar::new(progress.unwrap_or(0.0))
                                        .desired_width(260.0)
                                        .animate(true)
                                        .fill(p.accent),
                                );
                            }
                            Ok(Ok(Applied::Restart)) => {
                                ui.label(RichText::new(tr("update.installed")).color(p.success));
                                if crate::theme::primary_button(ui, tr("update.restart"), true)
                                    .clicked()
                                {
                                    restart();
                                }
                            }
                            Ok(Ok(Applied::Exit)) => {
                                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                            }
                            Ok(Ok(Applied::Manual)) => {
                                ui.label(tr("update.manual"));
                                if crate::theme::secondary_button(ui, tr("update.later")).clicked()
                                {
                                    self.install = None;
                                    self.dismissed = true;
                                }
                            }
                            Ok(Err(error)) => {
                                ui.label(
                                    RichText::new(trf("update.failed", &[("error", &error)]))
                                        .color(p.danger),
                                );
                                if let Some(release) = &release
                                    && crate::theme::secondary_button(ui, tr("update.open_page"))
                                        .clicked()
                                {
                                    ctx.open_url(egui::OpenUrl::new_tab(&release.page));
                                }
                                if crate::theme::secondary_button(ui, tr("update.later")).clicked()
                                {
                                    self.install = None;
                                    self.dismissed = true;
                                }
                            }
                        }
                        return;
                    }
                    let Some(release) = release else {
                        return;
                    };
                    ui.label(
                        RichText::new(trf(
                            "update.available",
                            &[
                                ("version", &release.version),
                                ("current", &current_version()),
                            ],
                        ))
                        .strong(),
                    );
                    let notes = ui.link(tr("update.notes"));
                    let notes = if release.notes.trim().is_empty() {
                        notes
                    } else {
                        notes.on_hover_text(release.notes.trim())
                    };
                    if notes.clicked() {
                        ctx.open_url(egui::OpenUrl::new_tab(&release.page));
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if crate::theme::secondary_button(ui, tr("update.later")).clicked() {
                            self.dismissed = true;
                        }
                        if crate::theme::primary_button(ui, tr("update.install"), true).clicked() {
                            let kind = Install::detect();
                            match kind.asset(&release).cloned() {
                                // Pas de fichier pour cette installation : la page de la release.
                                None => ctx.open_url(egui::OpenUrl::new_tab(&release.page)),
                                Some(asset) => {
                                    self.install = Some(Task::spawn(
                                        ctx,
                                        tr("update.downloading"),
                                        move |r| {
                                            let file = download(&release, &asset, &|f| {
                                                r.progress(Some(f), tr("update.downloading"))
                                            })?;
                                            r.progress(None, tr("update.installing"));
                                            apply(kind, &file)
                                        },
                                    ));
                                }
                            }
                        }
                    });
                });
            });
    }
}
