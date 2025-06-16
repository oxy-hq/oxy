import React from "react";
import { RegisterForm } from "./RegisterForm";

import OxyLogo from "@/components/OxyLogo";

const RegisterPage: React.FC = () => {
  return (
    <div className="bg-card grid h-full w-full overflow-auto customScrollbar">
      <div className="flex flex-col gap-4 p-6 md:p-10">
        <div className="flex justify-center gap-2 md:justify-start">
          <a href="#" className="flex items-center gap-2 font-medium">
            <OxyLogo />
            Oxy
          </a>
        </div>
        <div className="flex flex-1 items-center justify-center">
          <div className="w-full max-w-xs">
            <RegisterForm />
          </div>
        </div>
      </div>
    </div>
  );
};

export default RegisterPage;
