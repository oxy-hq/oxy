interface AppItem {
  name: string;
  path: string;
}

interface AppListProps {
  apps: AppItem[];
  selectedApp: AppItem | null;
  onSelectApp: (app: AppItem) => void;
}

export default function AppList({ apps, selectedApp, onSelectApp }: AppListProps) {
  if (apps.length === 0) {
    return (
      <div className="empty-state">
        <p>No apps found. Click "List Apps" to fetch available apps.</p>
      </div>
    );
  }

  return (
    <div className="app-list">
      {apps.map((app) => (
        <div
          key={app.path}
          className={`app-item ${selectedApp?.path === app.path ? 'selected' : ''}`}
          onClick={() => onSelectApp(app)}
        >
          <div className="app-name">{app.name}</div>
          <div className="app-path">{app.path}</div>
        </div>
      ))}
    </div>
  );
}
