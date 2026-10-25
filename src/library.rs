use crate::media::MediaReader;
use anyhow::Result;
use crossbeam_channel::{Receiver, Sender};
use rusqlite::{params, Connection};
use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
    thread,
};

#[derive(Clone, Debug)]
pub struct Track {
    pub path: PathBuf,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration: f64,
}
#[derive(Clone, Debug)]
pub struct Playlist {
    pub id: i64,
    pub name: String,
}
pub enum PlaylistTarget {
    Existing(i64),
    New(String),
}

pub enum Request {
    Scan(PathBuf),
    Query {
        text: String,
        offset: usize,
        show_all: bool,
        generation: u64,
    },
    ListPlaylists,
    LoadPlaylist(i64),
    SavePlaylist(i64, Vec<PathBuf>),
    CreatePlaylist(String),
    AddTracks(PlaylistTarget, Vec<PathBuf>),
    RenamePlaylist(i64, String),
    DeletePlaylist(i64),
    Shutdown,
}
pub enum Response {
    Page {
        tracks: Vec<Track>,
        total: usize,
        paths: Vec<PathBuf>,
        generation: u64,
    },
    Playlists(Vec<Playlist>),
    Playlist {
        id: i64,
        paths: Vec<PathBuf>,
    },
    TracksAdded {
        id: i64,
        paths: Vec<PathBuf>,
    },
    ScanProgress {
        processed: usize,
        errors: usize,
        done: bool,
    },
    Saved,
    Error(String),
}

pub struct Library {
    pub requests: Sender<Request>,
    pub responses: Receiver<Response>,
    worker: Option<thread::JoinHandle<()>>,
}
impl Library {
    pub fn start(path: PathBuf, ctx: eframe::egui::Context) -> Self {
        let (tx, rx) = crossbeam_channel::unbounded();
        let (results, responses) = crossbeam_channel::unbounded();
        let worker = thread::Builder::new()
            .name("music-library".into())
            .spawn(move || {
                if let Err(e) = run(path, rx, &results, &ctx) {
                    let _ = results.send(Response::Error(format!("音樂庫：{e:#}")));
                    ctx.request_repaint();
                }
            })
            .expect("library worker");
        Self {
            requests: tx,
            responses,
            worker: Some(worker),
        }
    }
    pub fn send(&self, r: Request) {
        let _ = self.requests.send(r);
    }
}
impl Drop for Library {
    fn drop(&mut self) {
        let _ = self.requests.send(Request::Shutdown);
        if let Some(w) = self.worker.take() {
            let _ = w.join();
        }
    }
}

fn initialize(connection: &Connection) -> Result<()> {
    connection.execute_batch("PRAGMA journal_mode=WAL;
        PRAGMA foreign_keys=ON;
        CREATE TABLE IF NOT EXISTS tracks(path TEXT PRIMARY KEY,title TEXT NOT NULL,artist TEXT NOT NULL,album TEXT NOT NULL,duration REAL NOT NULL,modified INTEGER NOT NULL);
        CREATE TABLE IF NOT EXISTS playlists(id INTEGER PRIMARY KEY,name TEXT NOT NULL);
        CREATE TABLE IF NOT EXISTS playlist_items(playlist_id INTEGER NOT NULL REFERENCES playlists(id) ON DELETE CASCADE,ordinal INTEGER NOT NULL,path TEXT NOT NULL,PRIMARY KEY(playlist_id,ordinal));
        INSERT OR IGNORE INTO playlists(id,name) VALUES(1,'我的音樂');")?;
    Ok(())
}

fn run(
    path: PathBuf,
    requests: Receiver<Request>,
    results: &Sender<Response>,
    ctx: &eframe::egui::Context,
) -> Result<()> {
    let mut db = Connection::open(path)?;
    initialize(&db)?;
    let mut scan: Option<walkdir::IntoIter> = None;
    let mut processed = 0;
    let mut errors = 0;
    let mut scan_roots = VecDeque::new();
    let mut pending = VecDeque::new();
    loop {
        while let Ok(r) = requests.try_recv() {
            pending.push_back(r);
        }
        if pending.is_empty() && scan.is_none() {
            match requests.recv() {
                Ok(r) => pending.push_back(r),
                Err(_) => break,
            }
        }
        while let Some(request) = pending.pop_front() {
            if matches!(request, Request::Shutdown) {
                return Ok(());
            }
            if let Request::Scan(root) = request {
                if scan.is_none() {
                    scan = Some(walkdir::WalkDir::new(root).follow_links(false).into_iter());
                    processed = 0;
                    errors = 0;
                } else {
                    scan_roots.push_back(root);
                }
                continue;
            }
            if let Err(e) = handle(&mut db, request, results) {
                let _ = results.send(Response::Error(format!("{e:#}")));
            }
            ctx.request_repaint();
        }
        if let Some(iterator) = &mut scan {
            match iterator.next() {
                Some(Ok(entry)) if entry.file_type().is_file() && is_audio(entry.path()) => {
                    if let Err(e) = index_file(&db, entry.path()) {
                        errors += 1;
                        if errors <= 3 {
                            let _ = results.send(Response::Error(format!(
                                "掃描 {}：{e:#}",
                                entry.path().display()
                            )));
                        }
                    }
                    processed += 1;
                    if processed % 16 == 0 {
                        let _ = results.send(Response::ScanProgress {
                            processed,
                            errors,
                            done: false,
                        });
                        ctx.request_repaint();
                    }
                }
                Some(Err(_)) => errors += 1,
                Some(_) => (),
                None => {
                    scan = scan_roots
                        .pop_front()
                        .map(|root| walkdir::WalkDir::new(root).follow_links(false).into_iter());
                    let _ = results.send(Response::ScanProgress {
                        processed,
                        errors,
                        done: scan.is_none(),
                    });
                    ctx.request_repaint();
                }
            }
        }
    }
    Ok(())
}

pub fn is_audio(path: &Path) -> bool {
    path.extension().is_some_and(|e| {
        matches!(
            e.to_string_lossy().to_ascii_lowercase().as_str(),
            "mp3" | "flac" | "wav" | "aac" | "m4a" | "ogg" | "opus"
        )
    })
}

fn index_file(db: &Connection, path: &Path) -> Result<()> {
    let path = path.canonicalize()?;
    let modified = path
        .metadata()?
        .modified()?
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs() as i64;
    let key = path.to_string_lossy();
    let old = db
        .query_row(
            "SELECT modified FROM tracks WHERE path=?1",
            [key.as_ref()],
            |r| r.get::<_, i64>(0),
        )
        .ok();
    if old == Some(modified) {
        return Ok(());
    }
    let reader = MediaReader::open(&path)?;
    let info = reader.info();
    db.execute("INSERT INTO tracks(path,title,artist,album,duration,modified) VALUES(?1,?2,?3,?4,?5,?6) ON CONFLICT(path) DO UPDATE SET title=excluded.title,artist=excluded.artist,album=excluded.album,duration=excluded.duration,modified=excluded.modified",
        params![key,info.title,info.artist,info.album,info.duration,modified])?;
    Ok(())
}

fn playlists(db: &Connection) -> Result<Vec<Playlist>> {
    Ok(db
        .prepare("SELECT id,name FROM playlists ORDER BY id")?
        .query_map([], |r| {
            Ok(Playlist {
                id: r.get(0)?,
                name: r.get(1)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?)
}

fn handle(db: &mut Connection, request: Request, tx: &Sender<Response>) -> Result<()> {
    match request {
        Request::Query {
            text,
            offset,
            show_all,
            generation,
        } => {
            let filter="instr(lower(title || char(10) || artist || char(10) || album || char(10) || path),lower(?1)) > 0";
            let paths = db
                .prepare(&format!(
                    "SELECT path FROM tracks WHERE {filter} ORDER BY title COLLATE NOCASE,path"
                ))?
                .query_map([&text], |r| Ok(PathBuf::from(r.get::<_, String>(0)?)))?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            let total = paths.len();
            let tracks=db.prepare(&format!("SELECT path,title,artist,album,duration FROM tracks WHERE {filter} ORDER BY title COLLATE NOCASE,path LIMIT ?3 OFFSET ?2"))?
                .query_map(params![text,offset,if show_all { -1i64 } else { 100 }],|r|Ok(Track{path:PathBuf::from(r.get::<_,String>(0)?),title:r.get(1)?,artist:r.get(2)?,album:r.get(3)?,duration:r.get(4)?}))?
                .collect::<std::result::Result<Vec<_>,_>>()?;
            let _ = tx.send(Response::Page {
                tracks,
                total,
                paths,
                generation,
            });
        }
        Request::ListPlaylists => {
            let _ = tx.send(Response::Playlists(playlists(db)?));
        }
        Request::LoadPlaylist(id) => {
            let paths = db
                .prepare("SELECT path FROM playlist_items WHERE playlist_id=?1 ORDER BY ordinal")?
                .query_map([id], |r| Ok(PathBuf::from(r.get::<_, String>(0)?)))?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            let _ = tx.send(Response::Playlist { id, paths });
        }
        Request::SavePlaylist(id, paths) => {
            let transaction = db.transaction()?;
            transaction.execute("DELETE FROM playlist_items WHERE playlist_id=?1", [id])?;
            {
                let mut statement = transaction.prepare(
                    "INSERT INTO playlist_items(playlist_id,ordinal,path) VALUES(?1,?2,?3)",
                )?;
                for (i, path) in paths.iter().enumerate() {
                    statement.execute(params![id, i, path.to_string_lossy()])?;
                }
            }
            transaction.commit()?;
            let _ = tx.send(Response::Saved);
        }
        Request::CreatePlaylist(name) => {
            if !name.trim().is_empty() {
                db.execute("INSERT INTO playlists(name) VALUES(?1)", [name.trim()])?;
            }
            let _ = tx.send(Response::Playlists(playlists(db)?));
        }
        Request::AddTracks(target, added) => {
            anyhow::ensure!(!added.is_empty(), "尚未選取曲目");
            let transaction = db.transaction()?;
            let id = match target {
                PlaylistTarget::Existing(id) => {
                    anyhow::ensure!(
                        transaction.query_row(
                            "SELECT EXISTS(SELECT 1 FROM playlists WHERE id=?1)",
                            [id],
                            |r| r.get::<_, bool>(0)
                        )?,
                        "清單已不存在"
                    );
                    id
                }
                PlaylistTarget::New(name) => {
                    anyhow::ensure!(!name.trim().is_empty(), "請輸入清單名稱");
                    transaction.execute("INSERT INTO playlists(name) VALUES(?1)", [name.trim()])?;
                    transaction.last_insert_rowid()
                }
            };
            let start: usize = transaction.query_row(
                "SELECT COALESCE(MAX(ordinal)+1,0) FROM playlist_items WHERE playlist_id=?1",
                [id],
                |r| r.get(0),
            )?;
            {
                let mut statement = transaction.prepare(
                    "INSERT INTO playlist_items(playlist_id,ordinal,path) VALUES(?1,?2,?3)",
                )?;
                for (offset, path) in added.iter().enumerate() {
                    statement.execute(params![id, start + offset, path.to_string_lossy()])?;
                }
            }
            transaction.commit()?;
            let paths = db
                .prepare("SELECT path FROM playlist_items WHERE playlist_id=?1 ORDER BY ordinal")?
                .query_map([id], |r| Ok(PathBuf::from(r.get::<_, String>(0)?)))?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            let _ = tx.send(Response::Playlists(playlists(db)?));
            let _ = tx.send(Response::TracksAdded { id, paths });
        }
        Request::RenamePlaylist(id, name) => {
            if !name.trim().is_empty() {
                db.execute(
                    "UPDATE playlists SET name=?2 WHERE id=?1",
                    params![id, name.trim()],
                )?;
            }
            let _ = tx.send(Response::Playlists(playlists(db)?));
        }
        Request::DeletePlaylist(id) => {
            if id != 1 {
                db.execute("DELETE FROM playlists WHERE id=?1", [id])?;
            }
            let _ = tx.send(Response::Playlists(playlists(db)?));
        }
        Request::Scan(_) | Request::Shutdown => (),
    }
    Ok(())
}

pub fn smoke_test(root: PathBuf) -> Result<()> {
    let expected = walkdir::WalkDir::new(&root)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file() && is_audio(e.path()))
        .count();
    anyhow::ensure!(expected > 0, "No audio fixtures");
    let db_path = root.join(format!("library-test-{}.sqlite", std::process::id()));
    let library = Library::start(db_path.clone(), eframe::egui::Context::default());
    let mut scans = 0;
    for _ in 0..2 {
        library.send(Request::Scan(root.clone()));
        loop {
            match library
                .responses
                .recv_timeout(std::time::Duration::from_secs(30))?
            {
                Response::ScanProgress {
                    processed,
                    errors,
                    done: true,
                } => {
                    anyhow::ensure!(
                        processed == expected && errors == 0,
                        "Scan mismatch: {processed}/{expected}, errors={errors}"
                    );
                    scans += 1;
                    break;
                }
                Response::Error(e) => anyhow::bail!(e),
                _ => (),
            }
        }
    }
    library.send(Request::Query {
        text: String::new(),
        offset: 0,
        show_all: false,
        generation: 1,
    });
    let paths = loop {
        match library
            .responses
            .recv_timeout(std::time::Duration::from_secs(5))?
        {
            Response::Page { tracks, total, .. } => {
                anyhow::ensure!(total == expected, "Duplicate indexing");
                break tracks.into_iter().map(|t| t.path).collect::<Vec<_>>();
            }
            Response::Error(e) => anyhow::bail!(e),
            _ => (),
        }
    };
    library.send(Request::SavePlaylist(1, paths.clone()));
    loop {
        if matches!(
            library
                .responses
                .recv_timeout(std::time::Duration::from_secs(5))?,
            Response::Saved
        ) {
            break;
        }
    }
    drop(library);
    let reopened = Library::start(db_path, eframe::egui::Context::default());
    reopened.send(Request::LoadPlaylist(1));
    match reopened
        .responses
        .recv_timeout(std::time::Duration::from_secs(5))?
    {
        Response::Playlist {
            paths: restored, ..
        } => anyhow::ensure!(restored == paths, "Playlist did not survive restart"),
        _ => anyhow::bail!("Unexpected response"),
    }
    println!(
        "{}",
        serde_json::json!({"library_test":"passed","indexed_tracks":expected,"scans":scans,"restart_restored":paths.len()})
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bulk_append_preserves_existing_order_duplicates_and_creates_atomically() {
        let mut db = Connection::open_in_memory().unwrap();
        initialize(&db).unwrap();
        let (tx, rx) = crossbeam_channel::unbounded();
        let a = PathBuf::from("C:/音樂/a, song.wav");
        let b = PathBuf::from("C:/音樂/b.wav");
        handle(&mut db, Request::SavePlaylist(1, vec![a.clone()]), &tx).unwrap();
        handle(
            &mut db,
            Request::AddTracks(PlaylistTarget::Existing(1), vec![b.clone(), a.clone()]),
            &tx,
        )
        .unwrap();
        let added = rx
            .try_iter()
            .find_map(|r| {
                if let Response::TracksAdded { paths, .. } = r {
                    Some(paths)
                } else {
                    None
                }
            })
            .unwrap();
        assert_eq!(added, vec![a.clone(), b.clone(), a.clone()]);
        handle(
            &mut db,
            Request::AddTracks(
                PlaylistTarget::New(" 新清單 ".into()),
                vec![b.clone(), a.clone()],
            ),
            &tx,
        )
        .unwrap();
        let list = playlists(&db).unwrap().pop().unwrap();
        assert_eq!(list.name, "新清單");
        let added = rx
            .try_iter()
            .find_map(|r| {
                if let Response::TracksAdded { id, paths } = r {
                    Some((id, paths))
                } else {
                    None
                }
            })
            .unwrap();
        assert_eq!(added, (list.id, vec![b, a.clone()]));
        assert!(handle(
            &mut db,
            Request::AddTracks(PlaylistTarget::New("  ".into()), vec![a.clone()]),
            &tx
        )
        .is_err());
        assert!(handle(
            &mut db,
            Request::AddTracks(PlaylistTarget::Existing(999), vec![a]),
            &tx
        )
        .is_err());
        assert_eq!(playlists(&db).unwrap().len(), 2);
    }

    #[test]
    fn playlists_round_trip_order_and_delete_without_files() {
        let mut db = Connection::open_in_memory().unwrap();
        initialize(&db).unwrap();
        let (tx, rx) = crossbeam_channel::unbounded();
        handle(&mut db, Request::CreatePlaylist("測試".into()), &tx).unwrap();
        let id = playlists(&db).unwrap().last().unwrap().id;
        let paths = vec![
            PathBuf::from("C:/音樂/b.mp3"),
            PathBuf::from("C:/音樂/a.mp3"),
        ];
        handle(&mut db, Request::SavePlaylist(id, paths.clone()), &tx).unwrap();
        handle(&mut db, Request::LoadPlaylist(id), &tx).unwrap();
        let found = rx
            .try_iter()
            .find_map(|r| {
                if let Response::Playlist { paths, .. } = r {
                    Some(paths)
                } else {
                    None
                }
            })
            .unwrap();
        assert_eq!(paths, found);
        handle(&mut db, Request::DeletePlaylist(id), &tx).unwrap();
        assert_eq!(
            db.query_row("SELECT count(*) FROM playlist_items", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
        handle(&mut db, Request::DeletePlaylist(1), &tx).unwrap();
        assert_eq!(playlists(&db).unwrap().len(), 1);
    }
    #[test]
    fn ten_thousand_rows_are_paged_and_searchable() {
        let mut db = Connection::open_in_memory().unwrap();
        initialize(&db).unwrap();
        {
            let transaction = db.transaction().unwrap();
            for n in 0..10_000 {
                transaction
                    .execute(
                        "INSERT INTO tracks VALUES(?1,?2,'歌手','專輯',3.0,0)",
                        params![format!("C:/music/{n:05}.mp3"), format!("歌曲 {n:05}")],
                    )
                    .unwrap();
            }
            transaction.commit().unwrap();
        }
        let (tx, rx) = crossbeam_channel::unbounded();
        handle(
            &mut db,
            Request::Query {
                text: "".into(),
                offset: 9900,
                show_all: false,
                generation: 7,
            },
            &tx,
        )
        .unwrap();
        match rx.recv().unwrap() {
            Response::Page {
                tracks,
                total,
                generation,
                paths,
            } => {
                assert_eq!(total, 10_000);
                assert_eq!(paths.len(), 10_000);
                assert_eq!(tracks.len(), 100);
                assert_eq!(generation, 7);
            }
            _ => panic!("wrong response"),
        }
        handle(
            &mut db,
            Request::Query {
                text: String::new(),
                offset: 0,
                show_all: true,
                generation: 9,
            },
            &tx,
        )
        .unwrap();
        match rx.recv().unwrap() {
            Response::Page {
                tracks,
                paths,
                total,
                ..
            } => {
                assert_eq!(total, 10_000);
                assert_eq!(tracks.len(), 10_000);
                assert_eq!(
                    tracks.iter().map(|t| t.path.clone()).collect::<Vec<_>>(),
                    paths
                );
            }
            _ => panic!("wrong response"),
        }
        handle(
            &mut db,
            Request::Query {
                text: "歌曲 09999".into(),
                offset: 0,
                show_all: false,
                generation: 8,
            },
            &tx,
        )
        .unwrap();
        match rx.recv().unwrap() {
            Response::Page { tracks, total, .. } => {
                assert_eq!(total, 1);
                assert!(tracks[0].path.ends_with("09999.mp3"));
            }
            _ => panic!("wrong response"),
        }
    }
}
