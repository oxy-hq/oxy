import { Navigate, NavLink, Route, BrowserRouter as Router, Routes } from "react-router-dom";
import { css } from "styled-system/css";

import ChatPage from "@/pages/Chat";
import SystemPage from "@/pages/system";

import ProjectPage from "./pages/project";

function App() {
  const containerClass = css({
    fontFamily: "mono",
    w: "100vw",
    h: "100vh",
    zIndex: "2",
    bg: "token(colors.background)",
    p: "6",
    border: "2px solid token(colors.primary)",
    borderRadius: "md"
  });

  const navClass = css({
    mb: "4",
    borderBottom: "1px solid token(colors.primary)",
    pb: "2",
    display: "flex",
    gap: "3"
  });

  const linkActiveClass = css({ backgroundColor: "token(colors.accent)" });

  const navItemClass = css({
    color: "token(colors.primary)",
    p: "1",
    border: "1px solid token(colors.primary)",
    "&:hover": { backgroundColor: "token(colors.secondary)" }
  });

  return (
    <Router>
      <div className={containerClass}>
        <nav className={navClass}>
          <NavLink to='/chat' className={({ isActive }) => (isActive ? linkActiveClass : "")}>
            <div className={navItemClass}>
              <p>Chat</p>
            </div>
          </NavLink>
          <NavLink to='/system' className={({ isActive }) => (isActive ? linkActiveClass : "")}>
            <div className={navItemClass}>
              <p>System config</p>
            </div>
          </NavLink>
          <NavLink to='/project' className={({ isActive }) => (isActive ? linkActiveClass : "")}>
            <div className={navItemClass}>
              <p>Project</p>
            </div>
          </NavLink>
        </nav>
        <Routes>
          <Route path='/chat' element={<ChatPage />} />
          <Route path='/system' element={<SystemPage />} />
          <Route path='/project' element={<ProjectPage />} />
          <Route path='*' element={<Navigate to='/system' />} />
        </Routes>
      </div>
    </Router>
  );
}

export default App;

