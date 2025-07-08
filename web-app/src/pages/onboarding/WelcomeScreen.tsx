import { Button } from "@/components/ui/shadcn/button";
import { Navigate, useNavigate } from "react-router-dom";
import { useOnboardingState } from "./hooks/useOnboardingState";

const WelcomeScreen = () => {
  const navigate = useNavigate();
  const { isLoading, currentStep, isOnboarded } = useOnboardingState();

  if (isLoading) {
    return <div>Loading...</div>;
  }

  if (isOnboarded) {
    return <Navigate to="/" replace />;
  }

  if (currentStep != "token") {
    return <Navigate to="/onboarding/setup" replace />;
  }

  return (
    <div className="min-h-screen w-full bg-secondary flex items-center justify-center p-4">
      <div className="w-full max-w-4xl">
        <div className="text-center mb-8">
          <h1 className="text-4xl md:text-5xl font-bold text-gray-900 dark:text-white mb-4">
            Welcome to Oxy
          </h1>
          <p className="text-xl text-gray-600 dark:text-gray-300 max-w-2xl mx-auto">
            Connect your GitHub repository to get started with powerful data
            workflows and AI-driven insights.
          </p>
        </div>

        <div className="text-center">
          <Button
            onClick={() => navigate("/onboarding/setup")}
            size="lg"
            className="px-8 py-3 text-lg font-semibold bg-blue-600 hover:bg-blue-700 text-white rounded-lg shadow-lg hover:shadow-xl transition-all duration-300"
          >
            Get Started with GitHub
          </Button>

          <p className="text-sm text-gray-500 dark:text-gray-400 mt-4">
            You'll need a GitHub Personal Access Token to continue
          </p>
        </div>
      </div>
    </div>
  );
};

export default WelcomeScreen;
