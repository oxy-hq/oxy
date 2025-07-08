import { Loader2 } from "lucide-react";

const LoadingScreen = () => (
  <div className="w-full min-h-screen bg-secondary flex items-center justify-center">
    <div className="text-center">
      <Loader2 className="h-8 w-8 animate-spin text-green-600 mx-auto mb-4" />
      <p className="text-gray-600 dark:text-gray-400">
        Setting up your project...
      </p>
    </div>
  </div>
);

export default LoadingScreen;
