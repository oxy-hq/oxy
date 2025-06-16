import React, { useEffect, useState } from "react";
import { useSearchParams, useNavigate } from "react-router-dom";
import { useAuth } from "@/contexts/AuthContext";
import { Button } from "@/components/ui/shadcn/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/shadcn/card";
import { CheckCircle, XCircle, Loader2 } from "lucide-react";
import useEmailVerification from "@/hooks/api/useEmailVerification";

const EmailVerificationPage: React.FC = () => {
  const [searchParams] = useSearchParams();
  const navigate = useNavigate();
  const { login } = useAuth();
  const [message, setMessage] = useState("");
  const [noToken, setNoToken] = useState(false);

  const { mutate: verifyEmail, status } = useEmailVerification();

  useEffect(() => {
    const token = searchParams.get("token");

    if (!token) {
      setMessage("Invalid verification link. No token provided.");
      setNoToken(true);
      return;
    }

    if (status === "idle") {
      verifyEmail(
        { token },
        {
          onSuccess: (response) => {
            setMessage("Email verified successfully! You are now logged in.");

            localStorage.setItem("authToken", response.token);
            login(response.token, response.user);

            setTimeout(() => {
              navigate("/");
            }, 2000);
          },
          onError: (error) => {
            setMessage(
              "Failed to verify email. The token may be invalid or expired.",
            );
            console.error("Email verification error:", error);
          },
        },
      );
    }
  }, [searchParams, navigate, login, verifyEmail, status]);

  const handleBackToLogin = () => {
    navigate("/login");
  };

  return (
    <div className="min-h-screen w-full bg-background flex items-center justify-center p-4">
      <Card className="max-w-md w-full">
        <CardHeader className="text-center">
          <div className="flex justify-center mb-4">
            {(status === "idle" || status === "pending") && !noToken && (
              <Loader2 className="h-12 w-12 text-primary animate-spin" />
            )}
            {status === "success" && (
              <CheckCircle className="h-12 w-12 text-green-500" />
            )}
            {(status === "error" || noToken) && (
              <XCircle className="h-12 w-12 text-red-500" />
            )}
          </div>
          <CardTitle className="text-2xl">
            {(status === "idle" || status === "pending") &&
              !noToken &&
              "Verifying Email..."}
            {status === "success" && "Email Verified!"}
            {(status === "error" || noToken) && "Verification Failed"}
          </CardTitle>
          <CardDescription>{message}</CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          {status === "error" && (
            <div className="space-y-2">
              <Button onClick={handleBackToLogin} className="w-full">
                Back to Login
              </Button>
            </div>
          )}
          {status === "success" && (
            <div className="text-center text-sm text-muted-foreground">
              Redirecting to home page...
            </div>
          )}
        </CardContent>
      </Card>
    </div>
  );
};

export default EmailVerificationPage;
