"""Labeled employee utterances for scoring the upsell-intent classifier.

Realistic Poke House counter speech, employee side only. `is_upsell` is the
ground truth: True only for a proactive offer of a paid add-on / upgrade.
The hard negatives (item mentioned but not offered, order confirmation,
generic "anything else?") are the ones that separate a real classifier from
a keyword matcher.
"""
from __future__ import annotations

# (text, is_upsell, item)
FIXTURES: list[tuple[str, bool, str | None]] = [
    # --- upsell offers (positive) ---
    ("Would you like to add avocado to that for a dollar fifty?", True, "avocado"),
    ("Do you want to make that a large for just two dollars more?", True, "large size"),
    ("Can I get you a drink to go with your bowl?", True, "drink"),
    ("Want to add some extra protein? The salmon's really good today.", True, "extra protein"),
    ("We've got fresh mango — would you like to add that on top?", True, "mango"),
    ("You want to add a side of edamame with that?", True, "edamame"),
    ("Would you like to try our house spicy mayo on it?", True, "spicy mayo"),
    ("Can I interest you in a cookie or a dessert today?", True, "dessert"),
    ("For a dollar more you can double up the protein, want to do that?", True, "double protein"),
    ("Would you like to add seaweed salad on the side?", True, "seaweed salad"),
    ("Do you want to upgrade to the large bowl?", True, "large bowl"),
    ("Can I add a bottled water or a soda for you?", True, "drink"),

    # --- order-taking / confirming / logistics (hard negatives) ---
    ("Okay, so that's one salmon bowl with brown rice and edamame.", False, None),
    ("Anything else for you today?", False, None),
    ("Your total comes to fourteen twenty-five.", False, None),
    ("Hi, welcome to Poke House! What can I get started for you?", False, None),
    ("That'll be ready in just a couple minutes.", False, None),
    ("Can I get a name for the order?", False, None),
    ("So you wanted no onions on that, correct?", False, None),
    ("Next guest in line, I can help you over here.", False, None),

    # --- item MENTIONED but not offered (the trap cases) ---
    ("Sorry, we're actually out of avocado today.", False, None),
    ("The signature bowl already comes with avocado and edamame.", False, None),
    ("The avocado's extra ripe today, just so you know.", False, None),
    ("That sauce has a little bit of a kick to it.", False, None),

    # --- customer-echo / ambiguous (employee repeating a customer request) ---
    ("Got it, you'd like to add avocado — no problem.", False, None),
    ("Sure, one large bowl coming right up.", False, None),
]
