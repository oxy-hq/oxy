import Text from "@/components/ui/Typography/Text";
import { AgentConfig } from "@/components/AgentEditor/type";
import { useFormContext } from "react-hook-form";
import Button from "@/components/ui/Button";
import Icon from "@/components/ui/Icon";
import AnonymizerModal from "./AnonymizerModal";
import { hstack, vstack } from "styled-system/patterns";
import { listItemStyle } from "../styles";

const AnonymizerField = () => {
  const { setValue, watch } = useFormContext<AgentConfig>();

  const anonymize = watch("anonymize");

  return (
    <div className={vstack({ gap: "sm", alignItems: "stretch" })}>
      <div className={hstack({ justifyContent: "space-between" })}>
        <Text variant="label14Medium" color="primary">
          Anonymizer config
        </Text>

        {!anonymize && (
          <AnonymizerModal
            value={anonymize}
            onUpdate={(data) => {
              setValue("anonymize", data);
            }}
            trigger={
              <Button variant="ghost" content="icon">
                <Icon asset="add" />
              </Button>
            }
          />
        )}
      </div>

      {anonymize && (
        <div className={hstack({ gap: "sm" })}>
          <AnonymizerModal
            value={anonymize}
            onUpdate={(data) => {
              setValue("anonymize", data);
            }}
            trigger={
              <button className={listItemStyle}>
                <Text variant="label14Medium" color="primary">
                  {anonymize.keywords_file ??
                    anonymize.mapping_file ??
                    "No anonymizer config"}
                </Text>

                <Text variant="label14Medium" color="secondary">
                  {anonymize.keywords_file ? "keywords" : "mapping"}
                </Text>
              </button>
            }
          />
          <Button
            variant="ghost"
            content="icon"
            onClick={() => {
              setValue("anonymize", undefined);
            }}
          >
            <Icon asset="remove_minus" />
          </Button>
        </div>
      )}
    </div>
  );
};

export default AnonymizerField;
