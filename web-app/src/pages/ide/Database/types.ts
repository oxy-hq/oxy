export interface DatabaseSchema {
  name: string;
  tables: { name: string }[];
}

export interface DatabaseConnection {
  id: string;
  name: string;
  type?: string;
  synced?: boolean;
  schemas?: DatabaseSchema[];
}
