//! The Start menu's apps as JSON, [{name, id, icon, kind}] (Apps.cs, `nova-helper apps`).
//!   id   = AppUserModelID from the shell's AppsFolder (launched with shell:AppsFolder\<id>)
//!   kind = "png" (Store app logo file) | "file" (exe/lnk to take the icon from) | ""
//! Runs on its own single-threaded apartment, as the shell wants.
use std::{collections::HashMap, fs, path::Path};

use windows::{
    core::{Interface, HSTRING, PWSTR},
    Win32::{
        Foundation::MAX_PATH,
        Storage::FileSystem::WIN32_FIND_DATAW,
        System::{
            Com::{CoCreateInstance, CoInitializeEx, CoTaskMemFree, IPersistFile, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, STGM_READ},
            Registry::*,
        },
        UI::Shell::*,
    },
};

/// .NET Version.TryParse: 2 to 4 non-negative integer parts; missing parts sort below 0.
fn version(s: &str) -> Vec<i64> {
    let parts: Vec<Option<i64>> = s.split('.').map(|p| p.parse::<i64>().ok().filter(|n| *n >= 0)).collect();
    if (2..=4).contains(&parts.len()) && parts.iter().all(Option::is_some) {
        let mut v: Vec<i64> = parts.into_iter().map(Option::unwrap).collect();
        v.resize(4, -1);
        v
    } else {
        vec![0, 0, -1, -1]
    }
}

fn reg_string(key: HKEY, sub: &str, name: &str) -> Option<String> {
    let mut buf = [0u16; 1024];
    let mut size = (buf.len() * 2) as u32;
    let ok = unsafe { RegGetValueW(key, &HSTRING::from(sub), &HSTRING::from(name), RRF_RT_REG_SZ, None, Some(buf.as_mut_ptr() as *mut _), Some(&mut size)) };
    ok.is_ok().then(|| String::from_utf16_lossy(&buf[..(size as usize / 2).saturating_sub(1)]))
}

/// Package family name (lower case) -> install folder, from the per-user package repository.
fn packages() -> HashMap<String, String> {
    const ROOT: &str = r"Software\Classes\Local Settings\Software\Microsoft\Windows\CurrentVersion\AppModel\Repository\Packages";
    let mut map = HashMap::new();
    let mut versions: HashMap<String, Vec<i64>> = HashMap::new();
    unsafe {
        let mut key = HKEY::default();
        if RegOpenKeyExW(HKEY_CURRENT_USER, &HSTRING::from(ROOT), None, KEY_READ, &mut key).is_err() {
            return map;
        }
        let mut i = 0;
        loop {
            let mut name = [0u16; 512];
            let mut len = name.len() as u32;
            if RegEnumKeyExW(key, i, Some(PWSTR(name.as_mut_ptr())), &mut len, None, None, None, None).is_err() {
                break;
            }
            i += 1;
            let full = String::from_utf16_lossy(&name[..len as usize]);
            // full name: Name_Version_Arch_ResourceId_PublisherId  ->  family: Name_PublisherId
            let parts: Vec<&str> = full.split('_').collect();
            if parts.len() < 5 || parts[3].to_lowercase().starts_with("split.") {
                continue;
            }
            let family = format!("{}_{}", parts[0], parts[parts.len() - 1]).to_lowercase();
            let v = version(parts[1]);
            if versions.get(&family).is_some_and(|have| *have >= v) {
                continue; // keep the newest
            }
            let Some(folder) = reg_string(key, &full, "PackageRootFolder").filter(|f| !f.is_empty()) else { continue };
            if !Path::new(&folder).join("AppxManifest.xml").is_file() {
                continue;
            }
            map.insert(family.clone(), folder);
            versions.insert(family, v);
        }
        let _ = RegCloseKey(key);
    }
    map
}

fn known_folder(id: &windows::core::GUID) -> Option<String> {
    unsafe {
        let p = SHGetKnownFolderPath(id, KNOWN_FOLDER_FLAG(0), None).ok()?;
        let s = p.to_string().ok();
        CoTaskMemFree(Some(p.0 as _));
        s
    }
}

/// .lnk files under a folder, in the order .NET's EnumerateFiles(AllDirectories) gives them.
fn lnk_files(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    let mut subdirs = Vec::new();
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            subdirs.push(p);
        } else if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("lnk")) {
            out.push(p);
        }
    }
    for d in subdirs {
        lnk_files(&d, out);
    }
}

/// Start menu shortcuts by display name, lower case (first one wins).
fn links() -> HashMap<String, String> {
    let mut map = HashMap::new();
    for root in [known_folder(&windows::Win32::UI::Shell::FOLDERID_CommonPrograms), known_folder(&windows::Win32::UI::Shell::FOLDERID_Programs)].into_iter().flatten() {
        let mut files = Vec::new();
        lnk_files(Path::new(&root), &mut files);
        for f in files {
            let name = f.file_stem().map(|s| s.to_string_lossy().to_lowercase()).unwrap_or_default();
            map.entry(name).or_insert_with(|| f.to_string_lossy().into_owned());
        }
    }
    map
}

// --- a minimal XML reader for AppxManifest.xml: elements, attributes and text ---
struct Node {
    local: String,
    attrs: Vec<(String, String)>,
    children: Vec<usize>,
    text: String,
}

fn local_name(n: &str) -> &str {
    n.rsplit(':').next().unwrap_or(n)
}

fn unescape_xml(s: &str) -> String {
    s.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&apos;", "'").replace("&amp;", "&")
}

fn parse_xml(src: &str) -> Vec<Node> {
    let mut nodes: Vec<Node> = vec![Node { local: String::new(), attrs: vec![], children: vec![], text: String::new() }];
    let mut stack = vec![0usize];
    let mut rest = src;
    while let Some(lt) = rest.find('<') {
        let text = &rest[..lt];
        if let Some(&top) = stack.last() {
            nodes[top].text.push_str(&unescape_xml(text));
        }
        rest = &rest[lt..];
        if rest.starts_with("<!--") {
            rest = rest.find("-->").map(|i| &rest[i + 3..]).unwrap_or("");
            continue;
        }
        if rest.starts_with("<![CDATA[") {
            let end = rest.find("]]>").unwrap_or(rest.len());
            if let Some(&top) = stack.last() {
                nodes[top].text.push_str(&rest[9..end]);
            }
            rest = &rest[(end + 3).min(rest.len())..];
            continue;
        }
        if rest.starts_with("<?") || rest.starts_with("<!") {
            rest = rest.find('>').map(|i| &rest[i + 1..]).unwrap_or("");
            continue;
        }
        let Some(gt) = rest.find('>') else { break };
        let tag = &rest[1..gt];
        rest = &rest[gt + 1..];
        if let Some(_close) = tag.strip_prefix('/') {
            stack.pop();
            continue;
        }
        let self_closing = tag.ends_with('/');
        let tag = tag.trim_end_matches('/');
        let name_end = tag.find(|c: char| c.is_whitespace()).unwrap_or(tag.len());
        let mut attrs = Vec::new();
        let mut a = &tag[name_end..];
        while let Some(eq) = a.find('=') {
            let key = a[..eq].trim().to_string();
            let after = a[eq + 1..].trim_start();
            let q = after.chars().next().unwrap_or('"');
            let after = &after[1..];
            let end = after.find(q).unwrap_or(after.len());
            attrs.push((local_name(&key).to_string(), unescape_xml(&after[..end])));
            a = &after[(end + 1).min(after.len())..];
        }
        let idx = nodes.len();
        nodes.push(Node { local: local_name(&tag[..name_end]).to_string(), attrs, children: vec![], text: String::new() });
        if let Some(&top) = stack.last() {
            nodes[top].children.push(idx);
        }
        if !self_closing {
            stack.push(idx);
        }
    }
    nodes
}

fn attr<'a>(n: &'a Node, name: &str) -> Option<&'a str> {
    n.attrs.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str())
}

/// The best-looking logo in a package: 48 px unplated first, never high-contrast.
fn find_logo(location: &str, app_id: &str) -> String {
    let Ok(src) = fs::read_to_string(Path::new(location).join("AppxManifest.xml")) else { return String::new() };
    let nodes = parse_xml(src.trim_start_matches('\u{feff}'));
    let app_id = app_id.replace('\'', "");
    let visual = nodes.iter().find(|n| n.local == "Application" && attr(n, "Id") == Some(app_id.as_str())).and_then(|app| {
        app.children.iter().map(|c| &nodes[*c]).find(|c| c.local == "VisualElements" && attr(c, "Square44x44Logo").is_some()).and_then(|v| attr(v, "Square44x44Logo")).map(str::to_string)
    });
    let props = || nodes.iter().filter(|n| n.local == "Properties").flat_map(|p| p.children.iter().map(|c| &nodes[*c])).find(|c| c.local == "Logo").map(|l| l.text.clone());
    let logo = visual.or_else(props).unwrap_or_default();
    if logo.is_empty() {
        return String::new();
    }
    let logo_path = Path::new(&logo);
    let dir = Path::new(location).join(logo_path.parent().unwrap_or(Path::new("")));
    let base = logo_path.file_stem().map(|s| s.to_string_lossy().to_lowercase()).unwrap_or_default();
    let mut best: Option<String> = None;
    let mut best_score = -1;
    if let Ok(entries) = fs::read_dir(&dir) {
        for e in entries.flatten() {
            let n = e.file_name().to_string_lossy().to_lowercase();
            if !n.starts_with(&base) || !n.ends_with(".png") {
                continue;
            }
            let mut score = if n.contains("targetsize-48_altform-unplated") {
                10
            } else if n.contains("targetsize-64_altform-unplated") {
                9
            } else if n.contains("targetsize-48") {
                8
            } else if n.contains("altform-unplated") {
                7
            } else if n.contains("scale-200") {
                6
            } else if n.contains("scale-100") {
                5
            } else {
                1
            };
            if n.contains("contrast") {
                score = 0;
            }
            if score > best_score {
                best = Some(dir.join(e.file_name()).to_string_lossy().into_owned());
                best_score = score;
            }
        }
    }
    if let Some(b) = best {
        return b;
    }
    let direct = Path::new(location).join(&logo);
    if direct.is_file() { direct.to_string_lossy().into_owned() } else { String::new() }
}

/// What a .lnk points to (WScript.Shell's TargetPath).
fn shortcut_target(lnk: &str) -> String {
    unsafe {
        let Ok(link) = CoCreateInstance::<_, IShellLinkW>(&ShellLink, None, CLSCTX_INPROC_SERVER) else { return String::new() };
        let Ok(file) = link.cast::<IPersistFile>() else { return String::new() };
        if file.Load(&HSTRING::from(lnk), STGM_READ).is_err() {
            return String::new();
        }
        let mut buf = [0u16; MAX_PATH as usize];
        let mut fd = WIN32_FIND_DATAW::default();
        if link.GetPath(&mut buf, &mut fd, SLGP_UNCPRIORITY.0 as u32).is_err() {
            return String::new();
        }
        let end = buf.iter().position(|c| *c == 0).unwrap_or(buf.len());
        String::from_utf16_lossy(&buf[..end])
    }
}

/// Apps.cs's string escaping: " and \ escaped, control characters as \uXXXX.
fn str_json(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn print() -> String {
    let packages = packages();
    let links = links();
    let mut rows = Vec::new();
    let mut seen = std::collections::HashSet::new();
    unsafe {
        let Ok(folder) = SHCreateItemFromParsingName::<_, _, IShellItem>(&HSTRING::from("shell:AppsFolder"), None) else { return "[]".into() };
        let Ok(items) = folder.BindToHandler::<_, IEnumShellItems>(None, &BHID_EnumItems) else { return "[]".into() };
        loop {
            let mut got = [None];
            let mut n = 0u32;
            if items.Next(&mut got, Some(&mut n)).is_err() || n == 0 {
                break;
            }
            let Some(item) = got[0].take() else { break };
            let name = item.GetDisplayName(SIGDN_NORMALDISPLAY).ok().map(|p| { let s = p.to_string().unwrap_or_default(); CoTaskMemFree(Some(p.0 as _)); s }).unwrap_or_default();
            let id = item.GetDisplayName(SIGDN_PARENTRELATIVEPARSING).ok().map(|p| { let s = p.to_string().unwrap_or_default(); CoTaskMemFree(Some(p.0 as _)); s }).unwrap_or_default();
            if name.is_empty() || id.is_empty() || !seen.insert(id.to_lowercase()) {
                continue;
            }
            let (mut icon, mut kind) = (String::new(), "");
            if let Some(bang) = id.find('!').filter(|b| *b > 0) {
                if let Some(loc) = packages.get(&id[..bang].to_lowercase()) {
                    icon = find_logo(loc, &id[bang + 1..]);
                    if !icon.is_empty() {
                        kind = "png";
                    }
                }
            }
            if icon.is_empty() {
                if let Some(lnk) = links.get(&name.to_lowercase()) {
                    let target = shortcut_target(lnk);
                    // a shortcut may point to a folder too (e.g. "XAMPP htdocs folder")
                    icon = if !target.is_empty() && Path::new(&target).exists() { target } else { lnk.clone() };
                    kind = "file";
                }
            }
            if icon.is_empty() && Path::new(&id).is_file() {
                icon = id.clone();
                kind = "file";
            }
            rows.push(format!("{{\"name\":{},\"id\":{},\"icon\":{},\"kind\":{}}}", str_json(&name), str_json(&id), str_json(&icon), str_json(kind)));
        }
    }
    format!("[{}]", rows.join(","))
}

/// The apps list (on a single-threaded apartment of its own).
pub fn scan() -> String {
    std::thread::spawn(|| {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        }
        print()
    })
    .join()
    .unwrap_or_else(|_| "[]".into())
}
