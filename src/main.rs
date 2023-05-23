use fs_extra::{dir, file};
use id3::{Tag, TagLike};
use std::{fs::DirEntry, path::{Path, PathBuf}};

fn main() {
    println!("lets move some folders!");
    let base = Path::new("tstfolder");
    movefolder(base);
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

fn main_move(folder: &Path, folder_year: &i32, genre: &str, music_base: &Path) {
    folder.read_dir().unwrap().for_each(|x| {
        println!("checking main_move folder: {:?}", x);
        let path = x.unwrap().path();
        if path.is_dir() {
            let _ = subfolder_move(&path, folder_year, genre, music_base);
        } else if file_is_song(&path) {
            println!("moving song: {:?}", path);
        } else {
            println!("not moving: {:?}", path);
        }
        remove_empty_folders(folder);
    });
}

fn subfolder_move(folder: &Path, folder_year: &i32, genre: &str, music_base: &Path) -> bool {
    let mut stays = false;
    println!("checking subfolder_move folder: {:?}", folder);

    folder.read_dir().unwrap().for_each(|x| {
        let path = x.unwrap().path();
        if path.is_dir() {
            stays = subfolder_move(&path, folder_year, genre, music_base);
        } else if file_is_deletable(&path) {
            println!("deleting file: {:?}", path);
            std::fs::remove_file(&path).unwrap();
        } else if !file_is_song(&path) && !file_is_deletable(&path) {
            println!("This got through cracks: {:?}", path)
        }
    });

    remove_empty_folders(folder);
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
        return true;
    }

    if maxyear != *folder_year && stays != true {
        let basefolder = make_base_year_genre_folder(&maxyear, genre, music_base);
        
        println!("moving folder: {:?} to {:?}", folder, basefolder);
        let options = fs_extra::dir::CopyOptions::new();
        let res = dir::move_dir(folder, basefolder, &options);
        match res {
            Ok(_) => (),
            Err(e) => println!("error moving folder: {:?}", e),
        }
        return false;
    } else {
        return true;
    }
}

fn make_base_year_genre_folder(year: &i32, genre: &str, music_base: &Path) -> PathBuf {
    let yearfolder = music_base.join(Path::new(&format!("{}-{}", genre, year)));
    if !yearfolder.exists() {
        dir::create(&yearfolder, true).unwrap();
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
    let yeartagopt = tag.get("TDRC").and_then(|frame| frame.content().text());
    match yeartagopt {
        Some(yeartag) => return yeartag.parse::<i32>().unwrap(),
        None => return 0,
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
