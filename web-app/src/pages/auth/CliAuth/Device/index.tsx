import type React from "react";
import { useSearchParams } from "react-router-dom";
import { isFramed } from "..";
import CliAuthCard from "../components/CliAuthCard";
import DeviceAuthorize from "./components/DeviceAuthorize";
import DeviceCodeEntry from "./components/DeviceCodeEntry";
import { cleanUserCode } from "./deviceRequest";

/**
 * `/cli-auth/device` — where a person approves `oxyc login --no-browser` and
 * `oxyc tokens create --agent|--sandbox-agent --no-browser` from any device: oxyc runs where no
 * browser reaches it (a cloud container, CI, a remote box), prints this URL with a code, and
 * polls until the request is decided.
 *
 * `?code=BCDF-GHJK` opens that request; without one, the person types the code. Like
 * `/cli-auth`, every state hands a credential to a program, so the page refuses to run in a
 * frame.
 */
const CliAuthDevice: React.FC = () => {
  const [params] = useSearchParams();
  const raw = params.get("code");
  const userCode = raw === null ? null : cleanUserCode(raw);

  if (isFramed()) {
    return (
      <CliAuthCard
        status='error'
        title='Open this page in its own tab'
        description='This page asks you to approve a sign-in, so it does not work inside another page. Nothing was sent.'
      />
    );
  }
  if (!userCode) return <DeviceCodeEntry invalid={raw !== null} key={raw ?? ""} />;
  // Keyed by the code, so typing another one starts that request's approval from the top.
  return <DeviceAuthorize userCode={userCode} key={userCode} />;
};

export default CliAuthDevice;
