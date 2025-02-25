import {
  Navigate,
  Route,
  BrowserRouter as Router,
  Routes,
} from "react-router-dom";
import { css } from "styled-system/css";

import "@xyflow/react/dist/style.css";
import WithSidebarLayout from "@/components/WithSidebarLayout";
import WorkflowPage from "./pages/worflow";

import ProtectedRoute from "./components/ProtectedRoute";
import { TooltipProvider } from "./components/ui/Tooltip";
import EmptyPage from "./pages/empty";
import Init from "./pages/init";
import FilePage from "./pages/file";
import AgentPageWrapper from "./pages/agent";
const TOOLTIP_DELAY_DURATION = 3;

const layoutWrapperStyles = css({
  display: "flex",
  flexDirection: "row",
  width: "100dvw",
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
              <Route path="/workflow/:pathb64" element={<WorkflowPage />} />
              <Route path="/agent/:pathb64" element={<AgentPageWrapper />} />
              <Route path="/file/:pathb64" element={<FilePage />} />
            </Route>
            <Route path="*" element={<Navigate to="/" />} />
          </Routes>
        </TooltipProvider>
      </div>
    </Router>
  );
}

export default App;
