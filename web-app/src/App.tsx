import React, { useEffect, useRef, useState } from "react";

import { useMutation } from "@tanstack/react-query";
import { ChatCompletionMessageParam } from "openai/resources/chat/completions";
import { css } from "styled-system/css";

const RetroChatApp = () => {
  const [messages, setMessages] = useState<ChatCompletionMessageParam[]>([]);
  const [input, setInput] = useState("");
  const [isLoading, setIsLoading] = useState(false);
  const messagesEndRef = useRef<HTMLDivElement>(null);

  const mutation = useMutation({
    mutationFn: async (messages: ChatCompletionMessageParam[]) => {
      const response = await fetch("https://api.openai.com/v1/chat/completions", {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
          Authorization: `Bearer ${import.meta.env.VITE_OPENAI_API_KEY}`
        },
        body: JSON.stringify({
          model: "gpt-3.5-turbo",
          messages,
          stream: true
        })
      });
      return response;
    }
  });

  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [messages]);

  const processStreamChunk = async (
    chunk: string,
    accumulatedContent: string,
    setMessages: React.Dispatch<React.SetStateAction<ChatCompletionMessageParam[]>>
  ) => {
    const lines = chunk.split("\n");
    let newContent = accumulatedContent;

    for (const line of lines) {
      if (!line.startsWith("data: ")) continue;

      const jsonData = line.slice(6);
      if (jsonData === "[DONE]") break;

      try {
        const data = JSON.parse(jsonData);
        const content = data.choices[0]?.delta?.content || "";
        if (content) {
          newContent += content;
          setMessages((prev) => {
            const newMessages = [...prev];
            const lastMessage = newMessages[newMessages.length - 1];
            if (lastMessage.role === "assistant") {
              lastMessage.content = newContent;
            }
            return newMessages;
          });
        }
      } catch (error) {
        console.error("Error parsing JSON:", error);
      }
    }

    return newContent;
  };

  const handleStreamResponse = async (response: Response) => {
    const reader = response.body?.getReader();
    const decoder = new TextDecoder();

    setMessages((prev) => [...prev, { role: "assistant", content: "" }]);

    let accumulatedContent = "";
    while (true) {
      const { done, value } = (await reader?.read()) ?? { done: true, value: undefined };
      if (done) break;

      const chunk = decoder.decode(value, { stream: true });
      accumulatedContent = await processStreamChunk(chunk, accumulatedContent, setMessages);
    }
  };

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!input.trim()) return;

    const userMessage: ChatCompletionMessageParam = { role: "user", content: input };
    setMessages((prev) => [...prev, userMessage]);
    setInput("");
    setIsLoading(true);

    try {
      const response = await mutation.mutateAsync([...messages, userMessage]);
      await handleStreamResponse(response);
    } catch (error) {
      console.error("Error:", error);
    } finally {
      setIsLoading(false);
    }
  };

  return (
    <div
      className={css({
        minH: "100vh",
        minW: "100vW",
        bg: "#0a1f1c",
        p: "4",
        fontFamily: "mono",
        color: "#00ff00",
        position: "relative",
        overflow: "hidden",
        "&::before": {
          content: '""',
          position: "fixed",
          top: "-2000px",
          left: "-2000px",
          right: "-2000px",
          bottom: "-2000px",
          background: "url('http://assets.iceable.com/img/noise-transparent.png') repeat",
          opacity: "0.9",
          animation: "noise 0.2s infinite"
        }
      })}
    >
      <div
        className={css({
          maxW: "4xl",
          mx: "auto",
          zIndex: "2"
        })}
      >
        <div
          className={css({
            mb: "4",
            p: "6",
            border: "2px solid #00ff00",
            borderRadius: "md",
            bg: "#001a1a"
          })}
        >
          <div
            className={css({
              display: "flex",
              justifyContent: "space-between",
              alignItems: "center",
              pb: "2",
              mb: "2",
              borderBottom: "1px solid #00ff00",
              fontSize: "sm"
            })}
          >
            <div>ONYX PLC 2.4.00</div>
            <div>TERMINAL INFO: A-34-0.1</div>
          </div>

          <div
            className={css({
              h: "60vh",
              overflowY: "auto",
              mb: "4"
            })}
          >
            {messages.map((message, index) => (
              <div
                key={index + message.role}
                className={css({
                  mb: "4",
                  _last: { mb: 0 }
                })}
              >
                <div
                  className={css({
                    display: "flex",
                    gap: "2"
                  })}
                >
                  <span
                    className={css({
                      fontWeight: "bold",
                      color: message.role === "user" ? "#dcdfde" : "#00ff00"
                    })}
                  >
                    {message.role === "user" ? "YOU:" : "SYSTEM:"}
                  </span>
                  <span
                    className={css({
                      flex: "1",
                      color: message.role === "user" ? "#dcdfde" : "#00ff00"
                    })}
                  >
                    {typeof message.content === "string"
                      ? message.content.toUpperCase()
                      : JSON.stringify(message.content).toUpperCase()}
                  </span>
                </div>
              </div>
            ))}
            <div ref={messagesEndRef} />
          </div>

          <form
            onSubmit={handleSubmit}
            className={css({
              position: "relative"
            })}
          >
            <div
              className={css({
                display: "flex",
                gap: "2"
              })}
            >
              <input
                type='text'
                value={input}
                onChange={(e) => setInput(e.target.value)}
                disabled={isLoading}
                className={css({
                  w: "full",
                  p: "2",
                  bg: "transparent",
                  color: "#00ff00",
                  outline: "none",
                  caretColor: "#00ff00",
                  textTransform: "uppercase",
                  "&::placeholder": {
                    color: "rgba(0, 255, 0, 0.5)"
                  }
                })}
                placeholder='ENTER MESSAGE...'
              />
              <button
                type='submit'
                disabled={isLoading}
                className={css({
                  minW: "80px",
                  borderRadius: "0px",
                  px: "4",
                  py: "2",
                  backgroundColor: "#00ff00 !important",
                  color: "#001a1a !important"
                })}
              >
                SEND
              </button>
            </div>
            <div
              className={css({
                position: "absolute",
                bottom: "0",
                left: "0",
                right: "0",
                h: "1px",
                bg: "#00ff00"
              })}
            />
          </form>
        </div>

        <div
          className={css({
            textAlign: "center",
            fontSize: "xs",
            color: "rgba(0, 255, 0, 0.7)"
          })}
        >
          PROPERTY OF ONYX SYSTEMS INC.
        </div>
      </div>
    </div>
  );
};

export default RetroChatApp;

