import type React from "react";
import { createContext, useContext } from "react";
import { redirectToHome } from "@/libs/utils";
import type { AuthConfigResponse, UserInfo } from "@/types/auth";

interface AuthContextType {
  getUser: () => string | null;
  getToken: () => string | null;
  isAuthenticated: () => boolean;
  login: (token: string, user: UserInfo) => void;
  logout: () => void;
  authConfig: AuthConfigResponse;
  isLocalMode: boolean;
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

export const AuthProvider: React.FC<AuthProviderProps> = ({ children, authConfig }) => {
  const login = (newToken: string, newUser: UserInfo) => {
    localStorage.setItem("auth_token", newToken);
    localStorage.setItem("user", JSON.stringify(newUser));
  };

  const logout = () => {
    localStorage.removeItem("auth_token");
    localStorage.removeItem("user");
    sessionStorage.clear();
    redirectToHome();
  };

  const storedUser = () => localStorage.getItem("user");
  const storedToken = () => localStorage.getItem("auth_token");

  const isLocalMode = authConfig.mode === "local";

  const value: AuthContextType = {
    getUser: storedUser,
    getToken: storedToken,
    // In local mode the backend auto-authenticates a guest user, so the UI
    // should treat every session as authenticated regardless of localStorage.
    isAuthenticated: () => isLocalMode || (!!storedToken() && !!storedUser()),
    login,
    logout,
    authConfig,
    isLocalMode
  };

  return <AuthContext.Provider value={value}>{children}</AuthContext.Provider>;
};
