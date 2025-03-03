import { useEffect, useState } from "react";
import useConfig, { Database } from "@/stores/useConfig";
import DropdownField from "./DropdownField";

const DatabaseSelect: React.FC = (props) => {
  const [databases, setDatabases] = useState<Database[]>([]);
  const configStore = useConfig();

  useEffect(() => {
    const fetchWarehouses = async () => {
      const config = await configStore.getConfig();
      setDatabases(config.databases);
    };
    fetchWarehouses();
  }, [configStore]);

  const options = databases.map((database) => ({
    label: database.name,
    value: database.name,
  }));

  return <DropdownField {...props} options={options} />;
};

export default DatabaseSelect;
