import React, { useState } from "react";

import { open } from "@tauri-apps/plugin-dialog";
import { DirEntry, readDir } from "@tauri-apps/plugin-fs";
import { css } from "styled-system/css";

import Icon from "@/components/ui/Icon";

const ignoreDir = [
  ".db",
  ".db-default-retrieval",
  "output",
  "bigquery-sample.key",
  ".db",
  "onyx.log",
  ".DS_Store",
  ".gitignore",
  ".git",
  "README.md"
];

const ProjectPath: React.FC = () => {
  const [selectedFolder, setSelectedFolder] = useState<string>("");

  const [dirEntry, setDirEntry] = useState<DirEntry[]>([]);

  //   const handleFolderSelect = (event: React.ChangeEvent<HTMLInputElement>) => {
  //     const files = event.target.files;
  //     if (files && files.length > 0) {
  //       setSelectedFolder(files[0].webkitRelativePath.split("/")[0]);
  //     }
  //     // window.showDirectoryPicker();
  //   };

  //   const x = () => {
  //     const input = document.createElement("input");
  //     input.type = "file";
  //     input.webkitdirectory = true;

  //     window.showDirectoryPicker();

  // input.addEventListener("change", () => {
  //   const files = Array.from(input.files);
  //   console.log(files);
  // });
  // if ("showPicker" in HTMLInputElement.prototype) {
  //   input.showPicker();
  // } else {
  //   input.click();
  // }
  //   };

  //   async function saveFileWithFileSystemAPI() {
  //     const directoryHandle = await window.showDirectoryPicker();
  //     const fileHandle = await directoryHandle.getFileHandle("fileName.txt", { create: true });
  //     const writable = await fileHandle.createWritable();
  //     await writable.write("content");
  //     await writable.close();
  //   }

  const handleFolderSelect = async () => {
    try {
      const selected = await open({
        directory: true,
        multiple: false,
        title: "Select Project Folder"
      });

      if (selected) {
        // Handle the selected folder path
        console.log("Selected folder:", selected);
        const folderPath = Array.isArray(selected) ? selected[0] : selected;
        setSelectedFolder(folderPath);

        const entries = await readDir(folderPath);
        setDirEntry(entries);
        console.log("Entries:", entries);
      }
    } catch (error) {
      console.error("Error selecting folder:", error);
    }
  };

  return (
    <div>
      <form>
        {/* <input webkitdirectory='' type='file' onChange={handleFolderSelect} /> */}
        <button type='button' onClick={handleFolderSelect}>
          Select Folder
        </button>

        <ul>
          {dirEntry
            .filter((i) => !ignoreDir.includes(i.name))
            .map((dir) => (
              <DirTree dirEntry={dir} path={selectedFolder} />
            ))}
        </ul>
      </form>
    </div>
  );
};

export default ProjectPath;

const DirTree = ({ dirEntry, path }: { dirEntry: DirEntry; path: string }) => {
  const [open, setOpen] = useState(false);
  const [dirChildren, setDirChildren] = useState<DirEntry[]>([]);

  return (
    <li
      className={css({
        display: "flex",
        flexDirection: "column"
      })}
      key={dirEntry.name}
    >
      <div
        className={css({
          display: "flex",
          flexDirection: "row",
          padding: "sm",
          gap: "sm"
        })}
        onClick={async (e) => {
          e.preventDefault();

          if (!open) {
            const dirE = await readDir(path + "/" + dirEntry.name);
            setDirChildren(dirE);
          }
          setOpen(!open);
        }}
      >
        {dirEntry.isDirectory && <Icon asset={open ? "chevron_down" : "chevron_right"} />}

        {dirEntry.isFile && <Icon asset='file' />}
        {dirEntry.isDirectory && <Icon asset={open ? "folder_open" : "folder"} />}
        {dirEntry.name}
      </div>

      {open && dirEntry.isDirectory && (
        <ul
          className={css({
            marginLeft: "30px"
          })}
        >
          {dirChildren.map((child) => (
            <DirTree dirEntry={child} path={path} />
          ))}
        </ul>
      )}
    </li>
  );
};

declare module "react" {
  interface HTMLAttributes<T> extends AriaAttributes, DOMAttributes<T> {
    directory?: string;
    webkitdirectory?: string;
  }
}

declare global {
  interface Window {
    showDirectoryPicker: () => Promise<FileSystemDirectoryHandle>;
    showSaveFilePicker: (options: never) => Promise<FileSystemFileHandle>;
  }
}
