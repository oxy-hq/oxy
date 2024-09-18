"""
currently langfuse does not support tracking time to first token for langchain
we need to customize the callback handler to support this feature
"""
from typing import Any, Dict, List, Optional
from uuid import UUID

from langchain_core.outputs import (
    ChatGeneration,
    LLMResult,
)
from langfuse.callback import CallbackHandler
from langfuse.callback.langchain import _extract_raw_esponse, _get_timestamp, _parse_model, _parse_usage


class CustomLangfuseCallbackHandler(CallbackHandler):
    first_token_time = {}

    def on_llm_new_token(
        self,
        token: str,
        *,
        run_id: UUID,
        parent_run_id: Optional[UUID] = None,
        tags: Optional[List[str]] = None,
        metadata: Optional[Dict[str, Any]] = None,
        **kwargs: Any,
    ) -> Any:
        """Run on new LLM token. Only available when streaming is enabled."""
        # Nothing needs to happen here for langfuse. Once the streaming is done,
        self.log.debug(f"on llm new token: run_id: {run_id} parent_run_id: {parent_run_id}")
        if run_id not in self.first_token_time:
            self.first_token_time[run_id] = _get_timestamp()

    def on_llm_end(self, response: LLMResult, *, run_id: UUID, parent_run_id: UUID | None = None, **kwargs: Any) -> Any:
        try:
            self._log_debug_event("on_llm_end", run_id, parent_run_id, response=response, kwargs=kwargs)
            if run_id not in self.runs:
                raise Exception("Run not found, see docs what to do in this case.")
            else:
                generation = response.generations[-1][-1]
                completion_start_time = None
                if run_id in self.first_token_time:
                    completion_start_time = self.first_token_time[run_id]
                extracted_response = (
                    self._convert_message_to_dict(generation.message)
                    if isinstance(generation, ChatGeneration)
                    else _extract_raw_esponse(generation)
                )

                llm_usage = _parse_usage(response)

                # e.g. azure returns the model name in the response
                model = _parse_model(response)
                self.runs[run_id] = self.runs[run_id].end(
                    output=extracted_response,
                    usage=llm_usage,
                    version=self.version,
                    input=kwargs.get("inputs"),
                    model=model,
                    completion_start_time=completion_start_time,
                )

                self._update_trace_and_remove_state(run_id, parent_run_id, extracted_response)

        except Exception as e:
            self.log.exception(e)
