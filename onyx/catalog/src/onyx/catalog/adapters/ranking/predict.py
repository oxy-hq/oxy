from functools import partial
from math import exp
from typing import Any, cast

from dsp import ExperimentalAdapter
from dspy import Example, Predict, Prediction, ensure_signature, settings, signature_to_template
from onyx.catalog.adapters.ranking.openai import CompletionChoice, LogProb
from onyx.shared.logging import get_logger

logger = get_logger(__name__)
SCORE_FIELD = "score"


class RankingPredict(Predict):
    def forward(self, **kwargs):
        # Extract the three privileged keyword arguments.
        new_signature = ensure_signature(kwargs.pop("new_signature", None))
        signature = ensure_signature(kwargs.pop("signature", self.signature))
        demos = kwargs.pop("demos", self.demos)
        config = dict(**self.config, **kwargs.pop("config", {}))

        # Get the right LM to use.
        lm = kwargs.pop("lm", self.lm) or settings.lm
        assert lm is not None, "No LM is loaded."

        # If temperature is 0.0 but its n > 1, set temperature to 0.7.
        temperature = config.get("temperature")
        temperature = lm.kwargs["temperature"] if temperature is None else temperature

        num_generations = config.get("n")
        if num_generations is None:
            num_generations = lm.kwargs.get("n", lm.kwargs.get("num_generations", 1))

        if (temperature is None or temperature <= 0.15) and num_generations > 1:
            config["temperature"] = 0.7
            # print(f"#> Setting temperature to 0.7 since n={num_generations} and prior temperature={temperature}.")

        if new_signature is not None:
            signature = new_signature

        if not all(k in kwargs for k in signature.input_fields):  # type: ignore
            present = [k for k in signature.input_fields if k in kwargs]  # type: ignore
            missing = [k for k in signature.input_fields if k not in kwargs]  # type: ignore
            print(f"WARNING: Not all input fields were provided to module. Present: {present}. Missing: {missing}.")

        # PATCH: Custom generate function
        completions = new_generate(lm, signature, Example(demos=demos, **kwargs), **config)
        logger.debug(f"RankingPredict Completions: {completions}")
        pred = Prediction.from_completions(completions, signature=signature)

        if kwargs.pop("_trace", True) and settings.trace is not None:
            trace = settings.trace
            trace.append((self, {**kwargs}, pred))

        return pred


class RankingAdapter(ExperimentalAdapter):
    def __init__(self, *args, **kwargs):
        ranking_on_field = kwargs.pop("ranking_on_field")
        super().__init__(*args, **kwargs)
        self.ranking_on_field = ranking_on_field

    def __set_example_field(self, example: Example, field_name: str, value: str):
        example[field_name] = value
        if field_name == self.ranking_on_field and example.get("logprobs") is not None:
            logprops = cast(LogProb, example.get("logprobs"))
            del example["logprobs"]
            for logprob in logprops["content"]:
                if logprob["token"].strip().lower() == value.lower():
                    example[SCORE_FIELD] = exp(logprob["logprob"])
                    logger.debug(f"Setting score {example[SCORE_FIELD]} to {example}")
                    break

    def extract(
        self,
        example: Example | dict[str, Any],
        raw_pred: str,
    ) -> Example:
        """Extracts the answer from the LM raw prediction using the template structure

        Args:
            example (Union[Example, dict[str, Any]]): Contains the input variables that raw_pred was completed on.
            raw_pred (str): LM generated string

        Returns:
            Example: The example with the output variables filled in
        """
        example = Example(example)

        raw_pred = raw_pred.strip()
        parts = raw_pred.split("\n")
        adjusted_parts = []
        for part in parts:
            trimmed_part = part.strip()
            if trimmed_part:
                if adjusted_parts:
                    adjusted_parts.append("\n" + trimmed_part)
                else:
                    adjusted_parts.append(trimmed_part)
        raw_pred = "\n".join(adjusted_parts)

        idx = 0
        while idx < len(self.fields):
            if self.fields[idx].input_variable not in example or example[self.fields[idx].input_variable] is None:
                break
            idx += 1

        idx = min(idx, len(self.fields) - 1)
        while raw_pred != "" and idx < len(self.fields):
            if idx < len(self.fields) - 1:
                next_field_name = "\n" + self.fields[idx + 1].name
                offset = raw_pred.find(next_field_name)

                if offset >= 0:
                    if settings.release >= 20231003:  # type: ignore
                        # PATCH: Set the output field and score
                        self.__set_example_field(
                            example,
                            self.fields[idx].output_variable,
                            raw_pred[:offset].strip().rstrip("---").strip(),  # noqa: B005
                        )
                        raw_pred = raw_pred[offset + len(next_field_name) :].strip().rstrip("---").strip()  # noqa: B005
                    else:
                        field_name_parts = self.fields[idx].name.split()
                        start_pos = 0
                        for part in field_name_parts:
                            pos = raw_pred.find(part.strip())
                            if pos != -1:
                                start_pos = pos + len(part)
                            else:
                                break

                    # PATCH: Set the output field and score
                    self.__set_example_field(
                        example,
                        self.fields[idx].output_variable,
                        raw_pred[start_pos:offset].strip().rstrip("---").strip(),  # noqa: B005
                    )
                    raw_pred = raw_pred[offset + len(next_field_name) :].strip()
                    idx += 1
                else:
                    # PATCH: Set the output field and score
                    self.__set_example_field(
                        example,
                        self.fields[idx].output_variable,
                        raw_pred.strip().rstrip("---").strip(),  # noqa: B005
                    )

                    raw_pred = ""
                    idx += 1
                    break

            else:
                assert idx == len(self.fields) - 1, (idx, len(self.fields))

                if settings.release >= 20231003:  # type: ignore
                    # PATCH: Set the output field and score
                    self.__set_example_field(
                        example,
                        self.fields[idx].output_variable,
                        raw_pred.strip().rstrip("---").strip(),  # noqa: B005
                    )
                else:
                    field_name_parts = self.fields[idx].name.split()
                    start_pos = 0
                    for part in field_name_parts:
                        pos = raw_pred.find(part.strip())
                        if pos != -1:
                            start_pos = pos + len(part)
                        else:
                            break
                    # PATCH: Set the output field and score
                    self.__set_example_field(example, self.fields[idx].output_variable, raw_pred[start_pos:].strip())

                break

        return example


def new_generate(lm, signature, example, max_depth=6, **kwargs):
    kwargs["stop"] = tuple(kwargs.get("stop", [])) or ("\n---",)

    # PATCH: Add ranking_on_field to kwargs
    ranking_on_field = kwargs.pop("ranking_on_field", None)
    template_adapter = ExperimentalAdapter
    if ranking_on_field is not None:
        template_adapter = partial(RankingAdapter, ranking_on_field=ranking_on_field)
        kwargs["logprobs"] = True

    # Generate and extract the fields.
    template = signature_to_template(signature, adapter=template_adapter)
    prompt = template(example)
    raw_completions: list[CompletionChoice] = lm(prompt, **kwargs)

    # PATCH: skip if no relevant agent or no completions found
    if not raw_completions:
        return []

    completions = []

    # PATCH: Add logprobs to example
    for p in raw_completions:
        example["logprobs"] = p["logprobs"]
        example = template.extract(example, p["raw_completion"])
        completions.append(example)

    assert all(set(signature.input_fields).issubset(set(c.keys())) for c in completions), "Missing input keys."

    # Find the completions that are most complete.
    field_names = [field.input_variable for field in template.fields]
    for field_idx, key in enumerate(field_names):  # noqa: B007
        completions_ = [c for c in completions if key in c.keys() and c[key] is not None]
        completions = completions_ or completions
        if len(completions_) == 0:
            break

    # If none of the completions is completed (i.e., none has the final field set).
    if len(completions_) == 0:
        # Pick the first completion that has gone farthest.
        completion = completions[0]

        for field_idx_ in range(field_idx + 1, len(field_names)):
            if field_names[field_idx_] in completion:
                del completion[field_names[field_idx_]]

        # Recurse with greedy decoding.
        new_kwargs = {
            **kwargs,
            "n": 1,
            "temperature": 0.0,
            "ranking_on_field": ranking_on_field,
        }

        assert max_depth > 0
        return new_generate(lm, signature, completion, max_depth=max_depth - 1, **new_kwargs)

    # PATCH: Keep only output fields and score.
    score_field = SCORE_FIELD if ranking_on_field else None
    logger.debug(f"New Generate: {completions} with ranking_on_field: {ranking_on_field}")

    completions = [
        {k: v for k, v in c.items() if k in signature.output_fields or k == score_field} for c in completions
    ]

    return completions
