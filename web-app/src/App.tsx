import { Navigate, Route, BrowserRouter as Router, Routes } from "react-router-dom";
import { css } from "styled-system/css";

import LeftSidebar from "@/components/LeftSidebar";
import MobileTopBar from "@/components/MobileTopBar";
import ChatPage from "@/pages/chat";

// import HomePage from "@/pages/home";
import ProjectPath from "./pages/project-path";

const layoutWrapperStyles = css({
  display: "flex",
  flex: "1",
  flexDirection: "row",
  width: "100%",
  maxW: "100dvw",
  alignItems: "stretch",
  h: "100dvh"
});

function App() {
  return (
    <Router>
      <div className={layoutWrapperStyles}>
        <MobileTopBar />
        <LeftSidebar />
        <Routes>
          <Route path='/' element={<ProjectPath />} />
          <Route path='/chat/:id' element={<ChatPage />} />
          <Route path='/init' element={<ProjectPath />} />
          <Route path='*' element={<Navigate to='/' />} />
        </Routes>
      </div>
    </Router>
  );
}

export default App;
