import openai
from dsp.modules.gpt3 import completions_request
from dspy import OpenAI
from onyx.shared.logging import Logged
from typing_extensions import TypedDict


class LogProbContent(TypedDict):
    token: str
    logprob: float


class LogProb(TypedDict):
    content: list[LogProbContent]


class CompletionChoice(TypedDict):
    raw_completion: str
    logprobs: LogProb


class GPTRanking(Logged, OpenAI):
    # PATCH: openai completion response can be None so handle it properly
    def basic_request(self, prompt: str, **kwargs):
        kwargs = {**self.kwargs, **kwargs}
        if self.model_type == "chat":
            # caching mechanism requires hashable kwargs
            messages = [{"role": "user", "content": prompt}]
            if self.system_prompt:
                messages.insert(0, {"role": "system", "content": self.system_prompt})
            kwargs["messages"] = messages
            response = openai.chat.completions.create(**kwargs)
            if response is None:
                self.log.warning(f"Chat completion response is None: {kwargs}")
            else:
                response = response.model_dump()

        else:
            kwargs["prompt"] = prompt
            response = completions_request(**kwargs)

        return response

    def __call__(
        self,
        prompt: str,
        only_completed: bool = True,
        return_sorted: bool = False,
        **kwargs,
    ) -> list[CompletionChoice]:
        """Retrieves completions from GPT-3.

        Args:
            prompt (str): prompt to send to GPT-3
            only_completed (bool, optional): return only completed responses and ignores completion due to length. Defaults to True.
            return_sorted (bool, optional): sort the completion choices using the returned probabilities. Defaults to False.

        Returns:
            list[CompletionChoice]: list of completion choices
        """

        assert only_completed, "for now"
        assert return_sorted is False, "for now"

        response = self.request(prompt, **kwargs)

        if not response:
            return []

        self.log_usage(response)
        choices = response["choices"]

        completed_choices = [c for c in choices if c["finish_reason"] != "length"]

        if only_completed and len(completed_choices):
            choices = completed_choices

        # PATCH: Add logprobs to output
        completions: list[CompletionChoice] = [
            {
                "raw_completion": self._get_choice_text(c),
                "logprobs": c["logprobs"],
            }
            for c in choices
        ]

        return completions
