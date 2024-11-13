import { sva, RecipeVariantProps, cva, cx } from "styled-system/css";

const skeletonAvatarStyles = cva({
  base: {
    rightSlideAnimation: true,
    flexShrink: "0"
  },
  variants: {
    size: {
      default: {
        w: "3xl",
        h: "3xl",
        borderRadius: "rounded"
      },
      small: {
        w: "lg",
        h: "lg",
        borderRadius: "minimal"
      }
    }
  }
});

const skeletonStyles = sva({
  slots: ["root", "line"],
  base: {
    root: {
      width: "100%",
      display: "flex",
      flexDirection: "column"
    },
    line: {
      rightSlideAnimation: true,
      width: "100%"
    }
  },
  variants: {
    size: {
      default: {
        root: {},
        line: {
          h: "lg",
          margin: "sm"
        }
      },
      small: {
        root: {
          gap: "xs"
        },
        line: {
          height: "xs",
          margin: "none"
        }
      },
      large: {
        root: {
          gap: "sm"
        },
        line: {
          height: "56px"
        }
      }
    }
  }
});

type SkeletonStyledProps = RecipeVariantProps<typeof skeletonStyles>;
type SkeletonAvatarStyledProps = RecipeVariantProps<
  typeof skeletonAvatarStyles
>;

type SkeletonLoaderProps = SkeletonStyledProps & {
  lineCount?: number;
  className?: string;
  lineClassName?: string;
};

type SkeletonAvatarProps = object

export function SkeletonAvatar({
  size = "default"
}: SkeletonAvatarProps & SkeletonAvatarStyledProps) {
  return <div className={skeletonAvatarStyles({ size })} />;
}

export default function Skeleton({
  lineCount = 5,
  size = "default",
  className,
  lineClassName
}: SkeletonLoaderProps) {
  const styles = skeletonStyles({ size });

  return (
    <div className={cx(styles.root, className)}>
      {[...Array(lineCount)].map((_, index) => (
        // eslint-disable-next-line sonarjs/no-array-index-key
        <div key={index} className={cx(styles.line, lineClassName)}></div>
      ))}
    </div>
  );
}
