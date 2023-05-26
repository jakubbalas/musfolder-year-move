use fs_extra::dir::{self, create};
use id3::{Tag, TagLike};
use rand::prelude::*;
use std::{
    fs::{create_dir, rename, DirEntry},
    io::stdin,
    path::{Path, PathBuf},
};
use postgres::{Client, NoTls};
use std::result::Result;
use std::env;

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

fn main() -> Result<(),()> {
    let args: Vec<String> = env::args().collect();

    let mut client = Client::connect("host=localhost user=mmove password=mmove dbname=mmove", NoTls).unwrap();
    step_load_files(&client);
    Ok(())
}

fn step_load_files(client: &Client) {
    let mut basepath_input = String::new();
    let stdin = stdin();
    stdin.read_line(&mut basepath_input).unwrap();
    println!("Now lets load some files into the database!");
    let basepath = Path::new(&basepath_input.replace("\n", "").replace("\r", ""));
    base_folder_load(&basepath, client);
}

fn base_folder_load(music_base: &Path, client: &Client) {
    music_base.read_dir().unwrap().for_each(|x| {
        let x = x.unwrap();
        let path = x.path();

        if path.is_dir() && path.to_str().unwrap().contains("-q") {
            let mut bits = path.file_name().unwrap().to_str().unwrap().split("-");
            let base_folder_data = TopFolder {
                year: bits.nth(0).unwrap().parse::<i32>().unwrap(),
                genre: bits.nth(0).unwrap().to_string(),
                colpath: music_base.to_str().unwrap().to_string(),
            };

            println!("Going through a folder: {:?}", path);
            folder_load(&path, &base_folder_data, 0, client);
        }
    });    
}

fn folder_load(folder: &Path, base_folder_data: &TopFolder, depth_level: i8, client: &Client) {
    folder.read_dir().unwrap().for_each(|x| {
        println!("checking main_move folder: {:?}", x);
        let path = x.unwrap().path();
        if path.is_dir() {
            let next_depth = depth_level + 1;
            let _ = folder_load(&path, base_folder_data, next_depth, client);
        } else if file_is_song(&path) && depth_level == 0{
            println!("moving song: {:?}", path);
            let year: i32 = 0;
            client.execute(
                "INSERT INTO foldermove (filepath, depth, year, moved, genre, currentyear, isdir) VALUES ($1, $2, $3, $4, $5, $6, $7)",
                &[&folder.to_str(), &depth_level, &year, &false, &base_folder_data.genre, &base_folder_data.year, &false],
            );
        } else if file_is_deletable(&path){
            println!("not moving: {:?}", path);
            std::fs::remove_file(&path).unwrap();
        }
    });
    remove_empty_folders(folder);
    let year: i32 = 0;
    client.execute(
        "INSERT INTO foldermove (filepath, depth, year, moved, genre, currentyear, isdir) VALUES ($1, $2, $3, $4, $5, $6, $7)",
        &[&folder.to_str(), &depth_level, &year, &false, &base_folder_data.genre, &base_folder_data.year, &true],
    );
}


fn main_2() {
    println!("lets move some folders! Enter the base folder for music library");
    let mut basepath = String::new();
    let stdin = stdin();
    stdin.read_line(&mut basepath).unwrap();
    basepath = basepath.replace("\n", "").replace("\r", "");
    match basepath.to_lowercase().find("music") {
        None => {
            println!("not a music folder, exiting");
            return;
        }
        _ => (),
    }
    println!("Using music base: {:?}", basepath);
    movefolder(Path::new(&basepath));
    println!("done!");
}

fn movefolder(music_base: &Path) {
    music_base.read_dir().unwrap().for_each(|x| {
        let x = x.unwrap();
        let path = x.path();

        if path.is_dir() && path.to_str().unwrap().contains("-q") {
            let mut bits = path.file_name().unwrap().to_str().unwrap().split("-");
            let genre = bits.nth(0).unwrap();
            let year = bits.nth(0).unwrap().parse::<i32>().unwrap();
            println!("genre: {:?}, year: {:?}", genre, year);

            println!("Going through a folder: {:?}", path);
            main_move(&path, &year, &genre, &music_base);
        }
    });
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
        .map(|x| get_song_year(x.unwrap()))
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
                std::fs::remove_dir(&subpath).unwrap();
            }
        }
    });
}

fn get_song_year(x: DirEntry) -> i32 {
    let path = x.path();
    if !file_is_song(&path) {
        return 0;
    }
    let tag = Tag::read_from_path(&path.to_str().unwrap()).unwrap();
    match tag.year() {
        Some(year) => return year,
        None => (),
    }

    let yeartagopt = tag.get("TDRC").and_then(|frame| frame.content().text());
    match yeartagopt {
        Some(yeartag) => return yeartag.parse::<i32>().unwrap(),
        None => {
            println!("no year tag found in file: {:?}", path);
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
        movefolder(&basemus);
        assert!(newtest.exists());
    }
}
