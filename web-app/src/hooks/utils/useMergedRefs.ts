import React from "react";

type Ref<T> = React.Ref<T>;

function useMergedRefs<T>(ref1: Ref<T>, ref2: Ref<T>): Ref<T> {
  return (node) => {
    if (typeof ref1 === "function") {
      ref1(node);
    } else if (ref1) {
      (ref1 as React.MutableRefObject<T | null>).current = node;
    }
    if (typeof ref2 === "function") {
      ref2(node);
    } else if (ref2) {
      (ref2 as React.MutableRefObject<T | null>).current = node;
    }
  };
}

export default useMergedRefs;
