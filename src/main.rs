use fs_extra::dir::{self, create};
use id3::{Tag, TagLike};
use postgres::{Client, NoTls};
use rand::prelude::*;
use std::env;
use std::result::Result;
use log::{info, warn};
use std::{
    fs::{create_dir, rename, DirEntry},
    io::stdin,
    path::{Path, PathBuf},
};

struct MusicFile {
    path_str: String,
    filename: String,
    year: i32,
    genre: String,
    correct: bool,
    depth_level: i8,
    colpath: String,
}

struct TopFolder {
    year: i32,
    genre: String,
    colpath: String,
}

const YEAR_INIT : i32 = 9999;
const YEAR_UNKNOWN : i32 = 0;
const YEAR_ERR : i32 = 6666;

fn main() -> Result<(), ()> {
    let args: Vec<String> = env::args().collect();

    let mut client = Client::connect(
        "host=localhost user=mmove password=mmove dbname=mmove",
        NoTls,
    )
    .unwrap();
    //step_load_files(&mut client);
    step_load_years(&mut client);
    Ok(())
}

fn step_load_files(client: &mut Client) {
    let mut basepath_input = String::new();
    let stdin = stdin();
    stdin.read_line(&mut basepath_input).unwrap();
    let basepath_input = &basepath_input.replace("\n", "").replace("\r", "");
    println!("Now lets load some files into the database!");
    match basepath_input.to_lowercase().find("music") {
        None => {
            println!("not a music folder, exiting");
            return;
        }
        _ => (),
    }
    let basepath = Path::new(basepath_input);
    base_folder_load(&basepath, client);
}

fn base_folder_load(music_base: &Path, client: &mut Client) {
    music_base.read_dir().unwrap().for_each(|x| {
        let x = x.unwrap();
        let path = x.path();

        if path.is_dir() && path.to_str().unwrap().contains("-q") {
            let mut bits = path.file_name().unwrap().to_str().unwrap().split("-");
            let base_folder_data = TopFolder {
                genre: bits.nth(0).unwrap().to_string(),
                year: bits.nth(0).unwrap().parse::<i32>().unwrap(),
                colpath: music_base.to_str().unwrap().to_string(),
            };

            println!("Going through a folder: {:?}", path);
            folder_load(&path, &base_folder_data, 0, client);
        }
    });
}

fn folder_load(folder: &Path, base_folder_data: &TopFolder, depth_level: i32, client: &mut Client) {
    folder.read_dir().unwrap().for_each(|x| {
        let path = x.unwrap().path();
        if path.is_dir() {
            let next_depth = depth_level + 1;
            let _ = folder_load(&path, base_folder_data, next_depth, client);
        } else if file_is_song(&path) && depth_level == 0{
            let year: i32 = YEAR_INIT;
            client.execute(
                "INSERT INTO foldermove (filepath, depth, year, moved, genre, currentyear, isdir, collection_base) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
                &[&path.to_str(), &depth_level, &year, &false, &base_folder_data.genre, &base_folder_data.year, &false, &base_folder_data.colpath],
            ).unwrap();
        } else if file_is_deletable(&path){
            match std::fs::remove_file(&path) {
                Ok(_) => info!("Removed file: {:?}", path),
                Err(e) => warn!("Error removing file: {:?}, error: {:?}", path, e),
            }
        }
    });
    remove_empty_folders(folder);
    let year: i32 = YEAR_INIT;
    client.execute(
        "INSERT INTO foldermove (filepath, depth, year, moved, genre, currentyear, isdir, collection_base) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        &[&folder.to_str(), &depth_level, &year, &false, &base_folder_data.genre, &base_folder_data.year, &true, &base_folder_data.colpath],
    ).unwrap();
}

fn step_load_years(client: &mut Client) {
    println!("Now lets load some years into the database!");
    let query = "SELECT filepath FROM foldermove WHERE year = $1";
    let rows = client.query(query, &[&YEAR_INIT]).unwrap();
    let mut year = 0;
    for row in rows {
        let file_path_str: String = row.get(0);
        let file_path = Path::new(&file_path_str);
        if !file_path.exists() {
            println!("file doesn't exist: {:?}", file_path);
            year = YEAR_ERR;
        } else if file_path.is_dir() {
            year = get_folder_year(file_path);
        } else {
            year = get_song_year(&file_path);
        }
        let query = "UPDATE foldermove SET year = $1 WHERE filepath = $2";
        client.execute(query, &[&year, &file_path_str]).unwrap();
    }
}

fn step_create_year_genre_folders(client: &mut Client) {
    let q = "SELECT year, genre FROM foldermove WHERE moved = False AND year BETWEEN 1800 AND 2500 AND currentyear != year GROUP BY year, genre";
    let rows = client.query(q, &[]).unwrap();
    for row in rows {
        let year: i32 = row.get(0);
        let genre: String = row.get(1);
        let mut genre_path = PathBuf::from("/home/mmove/Music");
        genre_path.push(&genre);
        let mut year_path = genre_path.clone();
        year_path.push(&year.to_string());
        if !year_path.exists() {
            std::fs::create_dir_all(&year_path).unwrap();
        }
        let query = "UPDATE foldermove SET moved = True WHERE year = $1 AND genre = $2";
        client.execute(query, &[&year, &genre]).unwrap();
    }
}

fn step_move_items(client: &mut Client) {
    println!("Running step of the move");
    let query = " ";
}

fn subfolder_move(folder: &Path, folder_year: &i32, genre: &str, music_base: &Path) -> bool {
    let mut stays = false;
    println!("checking subfolder_move folder: {:?}", folder);

    folder.read_dir().unwrap().for_each(|x| {
        let path = x.unwrap().path();
        if path.is_dir() && path.read_dir().unwrap().into_iter().count() > 0 {
            stays = subfolder_move(&path, folder_year, genre, music_base);
        } else if file_is_deletable(&path) {
            println!("deleting file: {:?}", path);
            std::fs::remove_file(&path).unwrap();
        } else if !file_is_song(&path) && !file_is_deletable(&path) {
            println!("This got through cracks: {:?}", path)
        }
    });

    remove_empty_folders(folder);
    if folder.read_dir().unwrap().into_iter().count() == 0 {
        return false;
    }
    if stays {
        return stays;
    }

    let maxyear = folder
        .read_dir()
        .unwrap()
        .map(|x| get_song_year(&x.unwrap().path()))
        .max()
        .unwrap_or_default();

    if maxyear == 0 {
        println!("No max year found in folder: {:?}", folder);
        return true;
    }

    if !(maxyear != *folder_year && stays != true) {
        return true;
    }

    let basefolder = make_base_year_genre_folder(&maxyear, genre, music_base);
    let mussize = folder.read_dir().unwrap().into_iter().count();
    println!(
        "moving folder: {:?} of size {:?} to {:?}",
        folder, mussize, basefolder
    );
    let tst = basefolder.join(folder.file_name().unwrap());
    println!("Checking existence of {:?}", tst);

    safe_move_item(folder, basefolder.as_path());
    return false;
}

fn make_base_year_genre_folder(year: &i32, genre: &str, music_base: &Path) -> PathBuf {
    let yearfolder = music_base.join(Path::new(&format!("{}-{}", genre, year)));
    if !yearfolder.exists() {
        dir::create(&yearfolder, false).unwrap();
        println!("created folder: {:?}", yearfolder);
    }
    return yearfolder;
}

fn remove_empty_folders(folder: &Path) {
    folder.read_dir().unwrap().for_each(|x| {
        let subpath = x.unwrap().path();
        if subpath.is_dir() {
            let mut empty = true;
            subpath.read_dir().unwrap().for_each(|_| {
                empty = false;
            });
            if empty {
                println!("deleting empty folder: {:?}", subpath);
                match std::fs::remove_dir(&subpath) {
                    Ok(_) => info!("Deleted folder: {:?}", subpath),
                    Err(_) => warn!("Couldn't delete folder: {:?}", subpath)
                }
            }
        }
    });
}

fn get_folder_year(folder_path: &Path) -> i32 {
    folder_path
        .read_dir()
        .unwrap()
        .map(|x| get_song_year(&x.unwrap().path()))
        .max()
        .unwrap_or_default()  
}

fn get_song_year(song_path: &Path) -> i32 {
    if !file_is_song(&song_path) {
        return 0;
    }
    let tag_read = Tag::read_from_path(&song_path.to_str().unwrap());
    match tag_read {
        Ok(_) => (),
        Err(_) => {
            println!("no tag found in file: {:?}", song_path);
            return 0;
        }        
    }
    let tag = tag_read.unwrap();
    match tag.year() {
        Some(year) => return year,
        None => (),
    }

    let yeartagopt = tag.get("TDRC").and_then(|frame| frame.content().text());
    match yeartagopt {
        Some(yeartag) => {
            if yeartag.contains("-") {
                
            }
            println!("found year tag: {:?}", yeartag);
            return yeartag.parse::<i32>().unwrap()
        },
        None => {
            println!("no year tag found in file: {:?}", song_path);
            return 0;
        }
    }
}

fn file_is_song(path: &Path) -> bool {
    let ext = path.extension().unwrap_or_default();
    match ext.to_str().unwrap() {
        "mp3" => true,
        "flac" => true,
        "ogg" => true,
        _ => false,
    }
}

fn file_is_deletable(path: &Path) -> bool {
    let filename = path.file_name().unwrap().to_str().unwrap();
    match filename {
        ".DS_Store" => return true,
        _ => (),
    };

    let ext = path.extension().unwrap_or_default();
    match ext.to_str().unwrap() {
        "jpg" => true,
        "jpeg" => true,
        "png" => true,
        "txt" => true,
        "nfo" => true,
        "m3u" => true,
        _ => false,
    }
}

fn safe_move_item(from: &Path, to: &Path) {
    if !to.exists() {
        panic!("Destination base folder doesn't exist")
    }
    if from.is_file() {
        rename(from, to).unwrap();
    }
    if from.is_dir() {
        let folder_name = from.file_name().unwrap();
        let newpath = to.join(folder_name);
        if newpath.exists() {
            println!("Found duplicate for folder: {:?}", newpath);
            let newpath = to.join(Path::new(&format!(
                "{}-{}",
                folder_name.to_str().unwrap(),
                random::<u32>()
            )));
            create(&newpath, false).unwrap();
            rename(from, &newpath).unwrap();
        } else {
            create(&newpath, false).unwrap();
            rename(from, &newpath).unwrap();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn movingfolder() {
        let base = Path::new("tstfolder");
        let origin = base.join(Path::new("origin"));
        assert!(origin.exists());
        let newtest = base.join(Path::new("testres"));
        let mut copyopt = dir::CopyOptions::new();
        copyopt.copy_inside = true;
        dir::create(&newtest, true).unwrap();
        dir::copy(origin, &newtest, &copyopt).unwrap();
        let basemus = newtest.join(Path::new("origin"));
        let mut client = Client::connect(
            "host=localhost user=mmove password=mmove dbname=mmove",
            NoTls,
        )
        .unwrap();
        base_folder_load(&basemus, &mut client);
        step_load_years(&mut client);
        assert!(newtest.exists());
    }
}
