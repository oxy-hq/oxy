import { apiClient } from "./axios";
import { FileTreeModel, FileStatus } from "@/types/file";

export class FileService {
  static async getFileTree(
    projectId: string,
    branchId: string,
  ): Promise<FileTreeModel[]> {
    const response = await apiClient.get(
      `/${projectId}/files?branch=${branchId}`,
    );
    return response.data;
  }

  static async getFile(
    projectId: string,
    pathb64: string,
    branchId: string,
  ): Promise<string> {
    const response = await apiClient.get(
      `/${projectId}/files/${pathb64}?branch=${branchId}`,
    );
    return response.data;
  }

  static async getFileFromGit(
    projectId: string,
    pathb64: string,
    branchId: string,
    commit = "HEAD",
  ): Promise<string> {
    const response = await apiClient.get(
      `/${projectId}/files/${pathb64}/from-git?branch=${branchId}&commit=${commit}`,
    );
    return response.data;
  }

  static async saveFile(
    projectId: string,
    pathb64: string,
    data: string,
    branchId: string,
  ): Promise<void> {
    const response = await apiClient.post(
      `/${projectId}/files/${pathb64}?branch=${branchId}`,
      {
        data,
      },
    );
    return response.data;
  }

  static async createFile(
    projectId: string,
    branchId: string,
    pathb64: string,
  ): Promise<void> {
    const response = await apiClient.post(
      `/${projectId}/files/${pathb64}/new-file?branch=${branchId}`,
    );
    return response.data;
  }

  static async createFolder(
    projectId: string,
    pathb64: string,
    branchId: string,
  ): Promise<void> {
    const response = await apiClient.post(
      `/${projectId}/files/${pathb64}/new-folder?branch=${branchId}`,
    );
    return response.data;
  }

  static async deleteFile(
    projectId: string,
    pathb64: string,
    branchId: string,
  ): Promise<void> {
    const response = await apiClient.delete(
      `/${projectId}/files/${pathb64}/delete-file?branch=${branchId}`,
    );
    return response.data;
  }

  static async deleteFolder(
    projectId: string,
    pathb64: string,
    branchId: string,
  ): Promise<void> {
    const response = await apiClient.delete(
      `/${projectId}/files/${pathb64}/delete-folder?branch=${branchId}`,
    );
    return response.data;
  }

  static async renameFile(
    projectId: string,
    pathb64: string,
    newName: string,
    branchId: string,
  ): Promise<void> {
    const response = await apiClient.put(
      `/${projectId}/files/${pathb64}/rename-file?branch=${branchId}`,
      {
        new_name: newName,
      },
    );
    return response.data;
  }

  static async renameFolder(
    projectId: string,
    pathb64: string,
    newName: string,
    branchId: string,
  ): Promise<void> {
    const response = await apiClient.put(
      `/${projectId}/files/${pathb64}/rename-folder?branch=${branchId}`,
      {
        new_name: newName,
      },
    );
    return response.data;
  }

  static async getDiffSummary(
    projectId: string,
    branchId: string,
  ): Promise<FileStatus[]> {
    const response = await apiClient.get(
      `/${projectId}/files/diff-summary?branch=${branchId}`,
    );
    return response.data;
  }
}
