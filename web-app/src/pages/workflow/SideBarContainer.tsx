import { css } from "styled-system/css";

type Props = {
    children: React.ReactNode;
}

export const SideBarContainer = ({ children }: Props) => {
    return (
        <div
            className={css({
                width: "280px",
                borderLeft: "1px solid",
                borderLeftColor: "neutral.border.colorBorderSecondary",
                backgroundColor: "#fff",
                overflow: "auto"
            })}
        >
            {children}
        </div>
    );
};
