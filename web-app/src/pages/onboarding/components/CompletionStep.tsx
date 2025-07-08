interface CompletionStepProps {
  isCompletingOnboarding: boolean;
  onComplete: () => void;
}

export const CompletionStep = ({
  isCompletingOnboarding,
  onComplete,
}: CompletionStepProps) => {
  return (
    <div className="border rounded-lg p-6 bg-card">
      <div className="text-center">
        <div className="mb-4">
          <div className="w-16 h-16 mx-auto mb-4 bg-green-100 rounded-full flex items-center justify-center">
            <svg
              className="w-8 h-8 text-green-600"
              fill="none"
              stroke="currentColor"
              viewBox="0 0 24 24"
              xmlns="http://www.w3.org/2000/svg"
            >
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                strokeWidth={2}
                d="M5 13l4 4L19 7"
              />
            </svg>
          </div>
          <h3 className="text-xl font-semibold text-green-600 mb-2">
            You're All Set!
          </h3>
          <p className="text-muted-foreground mb-6">
            Congratulations! Your project setup is complete and you're ready to
            start building with Oxy.
          </p>
        </div>
        <button
          onClick={onComplete}
          disabled={isCompletingOnboarding}
          className="px-6 py-3 bg-primary text-primary-foreground rounded-md hover:bg-primary/90 font-medium disabled:opacity-50 disabled:cursor-not-allowed flex items-center justify-center mx-auto"
        >
          {isCompletingOnboarding ? (
            <>
              <div className="animate-spin rounded-full h-4 w-4 border-b-2 border-primary-foreground mr-2"></div>
              Completing Setup...
            </>
          ) : (
            "Continue to App"
          )}
        </button>
      </div>
    </div>
  );
};
