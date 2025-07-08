import { CheckCircle } from "lucide-react";

const SuccessHeader = () => (
  <div className="text-center mb-8">
    <div className="mx-auto mb-6 p-4 bg-green-100 dark:bg-green-900/30 rounded-full w-fit">
      <CheckCircle className="h-12 w-12 text-green-600 dark:text-green-400" />
    </div>
    <h1 className="text-4xl font-bold text-gray-900 dark:text-white mb-2">
      Setup Complete!
    </h1>
    <p className="text-xl text-gray-600 dark:text-gray-300">
      Your GitHub repository has been successfully connected and cloned locally.
    </p>
  </div>
);

export default SuccessHeader;
