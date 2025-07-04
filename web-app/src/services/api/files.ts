import { apiClient } from "./axios";
import { FileTreeModel } from "@/types/file";

export class FileService {
  static async getFileTree(): Promise<FileTreeModel[]> {
    const response = await apiClient.get("/files");
    return response.data;
  }

  static async getFile(pathb64: string): Promise<string> {
    const response = await apiClient.get("/files/" + pathb64);
    return response.data;
  }

  static async saveFile(pathb64: string, data: string): Promise<void> {
    const response = await apiClient.post("/files/" + pathb64, { data });
    return response.data;
  }

  static async createFile(pathb64: string): Promise<void> {
    const response = await apiClient.post(`/files/${pathb64}/new-file`);
    return response.data;
  }

  static async createFolder(pathb64: string): Promise<void> {
    const response = await apiClient.post(`/files/${pathb64}/new-folder`);
    return response.data;
  }

  static async deleteFile(pathb64: string): Promise<void> {
    const response = await apiClient.delete(`/files/${pathb64}/delete-file`);
    return response.data;
  }

  static async deleteFolder(pathb64: string): Promise<void> {
    const response = await apiClient.delete(`/files/${pathb64}/delete-folder`);
    return response.data;
  }

  static async renameFile(pathb64: string, newName: string): Promise<void> {
    const response = await apiClient.put(`/files/${pathb64}/rename-file`, {
      new_name: newName,
    });
    return response.data;
  }

  static async renameFolder(pathb64: string, newName: string): Promise<void> {
    const response = await apiClient.put(`/files/${pathb64}/rename-folder`, {
      new_name: newName,
    });
    return response.data;
  }
}
