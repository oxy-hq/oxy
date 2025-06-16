import React, { createContext, useContext } from "react";
import { AuthConfigResponse, UserInfo } from "@/types/auth";
import { handleLogout, redirectToHome } from "@/libs/utils";

interface AuthContextType {
  getUser: () => string | null;
  getToken: () => string | null;
  isAuthenticated: () => boolean;
  login: (token: string, user: UserInfo) => void;
  logout: () => void;
  authConfig: AuthConfigResponse;
}

const AuthContext = createContext<AuthContextType | undefined>(undefined);

export const useAuth = () => {
  const context = useContext(AuthContext);
  if (!context) {
    throw new Error("useAuth must be used within an AuthProvider");
  }
  return context;
};

interface AuthProviderProps {
  children: React.ReactNode;
  authConfig: AuthConfigResponse;
}

export const AuthProvider: React.FC<AuthProviderProps> = ({
  children,
  authConfig,
}) => {
  const login = (newToken: string, newUser: UserInfo) => {
    localStorage.setItem("auth_token", newToken);
    localStorage.setItem("user", JSON.stringify(newUser));
  };

  const logout = () => {
    if (!authConfig.is_built_in_mode) {
      handleLogout();
    } else {
      localStorage.clear();
      sessionStorage.clear();
      redirectToHome();
    }
  };

  const storedUser = () => localStorage.getItem("user");
  const storedToken = () => localStorage.getItem("auth_token");

  const value: AuthContextType = {
    getUser: storedUser,
    getToken: storedToken,
    isAuthenticated: () => !!storedToken() && !!storedUser(),
    login,
    logout,
    authConfig,
  };

  return <AuthContext.Provider value={value}>{children}</AuthContext.Provider>;
};
