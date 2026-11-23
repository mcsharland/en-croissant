use std::{
    fs::{File, OpenOptions},
    io::{self, BufRead, BufReader, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};

use dashmap::DashMap;

use crate::{error::Error, AppState};

const GAME_OFFSET_FREQ: usize = 100;

struct PgnParser {
    reader: BufReader<File>,
    line: String,
    game: String,
    start: u64,
}

impl PgnParser {
    fn new(file: File) -> Self {
        let mut reader = BufReader::new(file);
        let start = ignore_bom(&mut reader).unwrap_or(0);
        Self {
            reader,
            line: String::new(),
            game: String::new(),
            start,
        }
    }

    fn position(&mut self) -> io::Result<u64> {
        self.reader.stream_position()
    }

    fn offset_by_index(
        &mut self,
        n: usize,
        offsets: &DashMap<String, Vec<u64>>,
        file: &str,
    ) -> io::Result<()> {
        let offset_index = n / GAME_OFFSET_FREQ;
        let n_left = n % GAME_OFFSET_FREQ;
        let wrapped_pgn_offsets = offsets.get(file);
        if wrapped_pgn_offsets.is_none() {
            self.reader.seek(SeekFrom::Start(self.start))?;
            self.skip_games(n)?;
            return Ok(());
        }
        let pgn_offsets = wrapped_pgn_offsets.unwrap();

        if offset_index == 0 || offset_index < pgn_offsets.len() {
            let offset = match offset_index {
                0 => self.start,
                _ => pgn_offsets[offset_index - 1],
            };

            self.reader.seek(SeekFrom::Start(offset))?;

            self.skip_games(n_left)?;
        } else {
            self.reader.seek(SeekFrom::Start(self.start))?;
            self.skip_games(n)?;
        }

        Ok(())
    }

    /// Skip n games, and return the number of bytes read
    fn skip_games(&mut self, n: usize) -> io::Result<usize> {
        let mut new_game = false;
        let mut skipped = 0;
        let mut count = 0;
        let mut in_comment = false;

        if n == 0 {
            return Ok(0);
        }

        let mut line = String::new();
        loop {
            let res = self.reader.read_line(&mut line);
            if res.is_err() {
                continue;
            }
            let bytes = res.unwrap();
            skipped += bytes;
            if bytes == 0 {
                break;
            }
            let is_header = !in_comment && line.starts_with('[');
            for c in line.chars() {
                match c {
                    '{' => in_comment = true,
                    '}' => in_comment = false,
                    _ => {}
                }
            }
            if is_header {
                if new_game {
                    count += 1;
                    if count == n {
                        self.reader.seek(SeekFrom::Current(-(bytes as i64)))?;
                        break;
                    }
                    new_game = false;
                }
            } else {
                new_game = true;
            }
            line.clear();
        }
        Ok(skipped)
    }

    fn read_game(&mut self) -> io::Result<String> {
        let mut new_game = false;
        let mut in_comment = false;
        self.game.clear();
        loop {
            let res = self.reader.read_line(&mut self.line);
            if res.is_err() {
                continue;
            }
            let bytes = res.unwrap();
            if bytes == 0 {
                break;
            }
            let is_header = !in_comment && self.line.starts_with('[');
            for c in self.line.chars() {
                match c {
                    '{' => in_comment = true,
                    '}' => in_comment = false,
                    _ => {}
                }
            }
            if is_header {
                if new_game {
                    break;
                }
            } else {
                new_game = true;
            }
            self.game.push_str(&self.line);
            self.line.clear();
        }
        Ok(self.game.clone())
    }
}

fn ignore_bom(reader: &mut BufReader<File>) -> io::Result<u64> {
    let mut bom = [0; 3];
    reader.read_exact(&mut bom)?;
    if bom != [0xEF, 0xBB, 0xBF] {
        reader.seek(SeekFrom::Start(0))?;
        return Ok(0);
    }
    Ok(3)
}

#[tauri::command]
#[specta::specta]
pub async fn count_pgn_games(
    file: PathBuf,
    state: tauri::State<'_, AppState>,
) -> Result<i32, Error> {
    let files_string = file.to_string_lossy().to_string();

    let file = File::open(&file)?;

    let mut parser = PgnParser::new(file.try_clone()?);

    let mut offsets = Vec::new();

    let mut count = 0;

    while let Ok(skipped) = parser.skip_games(1) {
        if skipped == 0 {
            break;
        }
        count += 1;
        if count % GAME_OFFSET_FREQ as i32 == 0 {
            let cur_pos = parser.position()?;
            offsets.push(cur_pos);
        }
    }

    state.pgn_offsets.insert(files_string, offsets);
    Ok(count)
}

#[tauri::command]
#[specta::specta]
pub async fn read_games(
    file: PathBuf,
    start: i32,
    end: i32,
    state: tauri::State<'_, AppState>,
) -> Result<Vec<String>, Error> {
    let file_r = File::open(&file)?;

    let mut parser = PgnParser::new(file_r.try_clone()?);

    parser.offset_by_index(
        start as usize,
        &state.pgn_offsets,
        file.to_string_lossy().as_ref(),
    )?;

    let mut games: Vec<String> = Vec::with_capacity((end - start) as usize);

    for _ in start..=end {
        let game = parser.read_game()?;
        if game.is_empty() {
            break;
        }
        games.push(game);
    }
    Ok(games)
}

#[tauri::command]
#[specta::specta]
pub async fn delete_game(
    file: PathBuf,
    n: i32,
    state: tauri::State<'_, AppState>,
) -> Result<(), Error> {
    delete_game_file(&file, n, &state.pgn_offsets)
}

fn delete_game_file(file: &Path, n: i32, offsets: &DashMap<String, Vec<u64>>) -> Result<(), Error> {
    let file_r = File::open(file)?;

    let mut parser = PgnParser::new(file_r.try_clone()?);
    let file_key = file.to_string_lossy();

    parser.offset_by_index(n as usize, offsets, file_key.as_ref())?;

    let starting_bytes = parser.position()?;

    parser.skip_games(1)?;

    let mut file_w = OpenOptions::new().write(true).open(file)?;

    file_w.seek(SeekFrom::Start(starting_bytes))?;

    write_to_end(&mut parser.reader, &mut file_w)?;

    offsets.remove(file_key.as_ref());

    Ok(())
}

fn write_to_end<R: Read>(reader: &mut R, writer: &mut File) -> io::Result<()> {
    io::copy(reader, writer)?;
    let end = writer.stream_position()?;
    writer.set_len(end)?;
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn write_game(
    file_path: String,
    n: i32,
    pgn: String,
    state: tauri::State<'_, AppState>,
) -> Result<(), Error> {
    write_game_file(Path::new(&file_path), n, &pgn, &state.pgn_offsets)
}

fn write_game_file(
    file: &Path,
    n: i32,
    pgn: &str,
    offsets: &DashMap<String, Vec<u64>>,
) -> Result<(), Error> {
    if !file.exists() {
        File::create(file)?;
    }

    let mut file_r = File::open(file)?;
    let mut file_w = OpenOptions::new().write(true).open(file)?;

    let mut tmpf = tempfile::tempfile()?;
    io::copy(&mut file_r.try_clone()?, &mut tmpf)?;

    file_r.seek(SeekFrom::Start(0))?;
    let mut parser = PgnParser::new(file_r.try_clone()?);
    let file_key = file.to_string_lossy();

    parser.offset_by_index(n as usize, offsets, file_key.as_ref())?;

    tmpf.seek(SeekFrom::Start(parser.position()?))?;
    tmpf.write_all(pgn.as_bytes())?;

    parser.skip_games(1)?;

    write_to_end(&mut parser.reader, &mut tmpf)?;

    tmpf.seek(SeekFrom::Start(0))?;

    write_to_end(&mut tmpf, &mut file_w)?;

    offsets.remove(file_key.as_ref());

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    const FIRST: &str =
        "[Event \"First\"]\n[Result \"1-0\"]\n\n1. e4 {Comment} e5 (1... c5) 1-0\n\n";
    const SECOND: &str = "[Event \"Second\"]\n[Result \"0-1\"]\n\n1. d4 d5 0-1\n\n";
    const THIRD: &str =
        "[Event \"Third\"]\n[Result \"1/2-1/2\"]\n\n1. c4 {Another comment} e5 1/2-1/2\n\n";
    const REPLACEMENT: &str =
        "[Event \"Replacement\"]\n[Result \"*\"]\n\n1. Nf3 d5 (1... Nf6) *\n\n";

    fn pgn_file(contents: &str) -> (TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("games.pgn");
        fs::write(&path, contents).unwrap();
        (dir, path)
    }

    #[test]
    fn replacing_game_preserves_other_games() {
        let (_dir, path) = pgn_file(&format!("{FIRST}{SECOND}{THIRD}"));
        let offsets = DashMap::new();

        write_game_file(&path, 1, REPLACEMENT, &offsets).unwrap();

        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            format!("{FIRST}{REPLACEMENT}{THIRD}")
        );
    }

    #[test]
    fn deleting_game_preserves_other_games() {
        for (index, expected) in [
            (0, format!("{SECOND}{THIRD}")),
            (1, format!("{FIRST}{THIRD}")),
            (2, format!("{FIRST}{SECOND}")),
        ] {
            let (_dir, path) = pgn_file(&format!("{FIRST}{SECOND}{THIRD}"));
            let offsets = DashMap::new();
            let key = path.to_string_lossy().into_owned();
            // both offsets are needed because game 150 requires the second cached entry,
            // with only one entry, the parser scans the file instead of using the cache
            offsets.insert(key.clone(), vec![]);

            delete_game_file(&path, index, &offsets).unwrap();

            assert_eq!(fs::read_to_string(&path).unwrap(), expected);
            assert!(!offsets.contains_key(&key));
        }
    }

    #[test]
    fn writing_game_creates_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("new.pgn");

        write_game_file(&path, 0, FIRST, &DashMap::new()).unwrap();

        assert_eq!(fs::read_to_string(&path).unwrap(), FIRST);
    }

    #[test]
    fn file_operations_report_io_errors() {
        let dir = tempfile::tempdir().unwrap();
        let offsets = DashMap::new();

        assert!(delete_game_file(&dir.path().join("missing.pgn"), 0, &offsets).is_err());
        assert!(write_game_file(&dir.path().join("missing/new.pgn"), 0, FIRST, &offsets).is_err());
    }

    #[test]
    fn editing_game_invalidates_cached_offsets() {
        let mut games: Vec<String> = (0..205)
            .map(|i| format!("[Event \"Game {i}\"]\n\n1. e4 e5 1-0\n\n"))
            .collect();
        let (_dir, path) = pgn_file(&games.concat());

        let mut parser = PgnParser::new(File::open(&path).unwrap());
        parser.skip_games(100).unwrap();
        let at_100 = parser.position().unwrap();
        parser.skip_games(100).unwrap();
        let at_200 = parser.position().unwrap();

        let offsets = DashMap::new();
        let key = path.to_string_lossy().into_owned();
        offsets.insert(key.clone(), vec![at_100, at_200]);

        // replacing an earlier game shifts the cached positions of later games
        write_game_file(&path, 1, REPLACEMENT, &offsets).unwrap();
        assert!(!offsets.contains_key(&key));
        games[1] = REPLACEMENT.to_string();

        delete_game_file(&path, 150, &offsets).unwrap();
        games.remove(150);

        assert_eq!(fs::read_to_string(&path).unwrap(), games.concat());
    }
}
