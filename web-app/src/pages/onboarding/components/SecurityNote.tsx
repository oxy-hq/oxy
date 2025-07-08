import { Check } from "lucide-react";

export const SecurityNote = () => {
  return (
    <div className="p-4 bg-green-50 dark:bg-green-900 border border-green-200 dark:border-green-800 rounded-lg">
      <div className="flex items-start space-x-3">
        <Check className="h-5 w-5 text-white mt-0.5" />
        <div>
          <p className="font-medium text-white">Your token is secure</p>
          <p className="text-sm text-white">
            We encrypt and store your token securely. It's never transmitted in
            plain text or shared with third parties.
          </p>
        </div>
      </div>
    </div>
  );
};
