import {
  Navigate,
  Route,
  BrowserRouter as Router,
  Routes,
} from "react-router-dom";
import { css } from "styled-system/css";

import WithSidebarLayout from "@/components/WithSidebarLayout";
import ChatPage from "@/pages/chat";

import ProtectedRoute from "./components/ProtectedRoute";
import { TooltipProvider } from "./components/ui/Tooltip";
import EmptyPage from "./pages/empty";
import Init from "./pages/init";

const TOOLTIP_DELAY_DURATION = 3;

const layoutWrapperStyles = css({
  display: "flex",
  flex: "1",
  flexDirection: "row",
  width: "100%",
  maxW: "100dvw",
  alignItems: "stretch",
  h: "100dvh",
});

function App() {
  return (
    <Router>
      <div className={layoutWrapperStyles}>
        <TooltipProvider delayDuration={TOOLTIP_DELAY_DURATION}>
          <Routes>
            <Route path="/init" element={<Init />} />
            <Route
              element={
                <ProtectedRoute>
                  <WithSidebarLayout />
                </ProtectedRoute>
              }
            >
              <Route path="/" element={<EmptyPage />} />
              <Route path="/chat/:id" element={<ChatPage />} />
            </Route>
            <Route path="*" element={<Navigate to="/" />} />
          </Routes>
        </TooltipProvider>
      </div>
    </Router>
  );
}

export default App;
