import { textStyles } from "./text-styles";

export const theme = {
  breakpoints: {
    sm: "640px",
    md: "768px",
    lg: "1024px",
    xl: "1280px",
    "2xl": "1536px"
  },
  textStyles,
  tokens: {
    fonts: {
      Inter: {
        value: "var(--font-family-inter)"
      },
      GeistMono: {
        value: "var(--font-family-geist-mono)"
      }
    },
    colors: {
      "light-grey": {
        "1": { value: "#FFFFFF" },
        "2": { value: "#F8F8F8" },
        "3": { value: "#EBEBEB" },
        "4": { value: "#9998A6" },
        "5": { value: "#939393" },
        "6": { value: "#626166" },
        "7": { value: "#343434" },
        "8": { value: "#202020" },
        opacity: { value: "rgba(248,248,248,0.7)" }
      },
      "dark-grey": {
        "1": { value: "#FFFFFF" },
        "2": { value: "#CACBCF" },
        "3": { value: "#8A8B8D" },
        "4": { value: "#4A4C53" },
        "5": { value: "#28292E" },
        "6": { value: "#1D1E24" },
        "7": { value: "#131418" },
        "8": { value: "#15161A" },
        opacity: { value: "rgba(0,0,0,0.6)" }
      },
      "dark-grey-new": {
        "1": { value: "#FFFFFF" },
        "2": { value: "#D3D4DA" },
        "3": { value: "#9EA0A9" },
        "4": { value: "#54575F" },
        "5": { value: "#35363B" },
        "6": { value: "#22242A" },
        "7": { value: "#0E0E11" },
        "8": { value: "#050505" },
        opacity: { value: "rgba(84,87,95,0.3)" }
      },
      graph: {
        "1": { value: "#6065A8" },
        "2": { value: "#515695" },
        "3": { value: "#484C88" },
        "4": { value: "#3E427B" },
        "5": { value: "#363A6F" },
        "6": { value: "#2C3062" },
        "7": { value: "#252858" }
      },
      green: {
        "1": { value: "#E8F5F1" },
        "2": { value: "#1E9B70" },
        "3": { value: "#203030" }
      },
      orange: {
        "1": { value: "#FDF2EB" },
        "2": { value: "#ED8132" },
        "3": { value: "#3F2D24" }
      },
      red: {
        "1": { value: "#F2A6A6" },
        "2": { value: "#C04438" },
        "3": { value: "#812A22" },
        "4": { value: "#591D17" }
      },
      pink: {
        "2": {
          value: "#E2237E"
        }
      },
      blue: {
        "2": {
          value: "#5186EE"
        }
      }
    },
    spacing: {
      0: { value: "0px" },
      "0.5": { value: "2px" },
      "1": { value: "4px" },
      "2": { value: "8px" },
      "3": { value: "12px" },
      "4": { value: "16px" },
      "5": { value: "20px" },
      "6": { value: "24px" },
      "7": { value: "28px" },
      "8": { value: "32px" },
      "9": { value: "40px" }
    },
    radii: {
      sm: { value: "4px" },
      md: { value: "8px" },
      lg: { value: "12px" },
      xl: { value: "16px" }
    },
    sizes: {
      1: { value: "12px" },
      2: { value: "14px" },
      3: { value: "16px" },
      4: { value: "20px" },
      5: { value: "24px" },
      6: { value: "28px" },
      7: { value: "32px" },
      8: { value: "36px" },
      9: { value: "40px" },
      10: { value: "44px" },
      11: { value: "48px" },
      12: { value: "52px" },
      13: { value: "56px" }
    }
  },
  semanticTokens: {
    colors: {
      text: {
        primary: {
          value: {
            _light: "{colors.light-grey.7}",
            _dark: {
              base: "{colors.dark-grey.2}",
              _newTheme: "{colors.dark-grey-new.1}"
            }
          }
        },
        secondary: {
          value: {
            _light: "{colors.light-grey.4}",
            _dark: {
              base: "{colors.dark-grey.4}",
              _newTheme: "{colors.dark-grey-new.3}"
            }
          }
        },
        light: {
          value: {
            _light: "{colors.light-grey.6}",
            _dark: {
              base: "{colors.dark-grey.3}",
              _newTheme: "{colors.dark-grey-new.2}"
            }
          }
        },
        contrast: {
          value: {
            _light: "{colors.light-grey.1}",
            _dark: {
              base: "{colors.dark-grey.2}",
              _newTheme: "{colors.dark-grey-new.1}"
            }
          }
        },
        "less-contrast": {
          value: {
            _light: "{colors.light-grey.5}",
            _dark: {
              base: "{colors.dark-grey.3}",
              _newTheme: "{colors.dark-grey-new.3}"
            }
          }
        },
        disabled: {
          value: {
            _light: "{colors.light-grey.3}",
            _dark: {
              base: "{colors.dark-grey.3}",
              _newTheme: "{colors.dark-grey-new.4}"
            }
          }
        },
        error: {
          value: {
            _light: "{colors.red.2}",
            _dark: "{colors.red.2}"
          }
        },
        success: {
          value: {
            _light: "{colors.green.2}",
            _dark: "{colors.green.2}"
          }
        },
        progress: {
          value: {
            _light: "{colors.orange.2}",
            _dark: "{colors.orange.2}"
          }
        }
      },

      background: {
        primary: {
          value: {
            _light: "{colors.light-grey.1}",
            _dark: {
              base: "{colors.dark-grey.8}",
              _newTheme: "{colors.dark-grey-new.8}"
            }
          }
        },
        secondary: {
          value: {
            _light: "{colors.light-grey.2}",
            _dark: {
              base: "{colors.dark-grey.7}",
              _newTheme: "{colors.dark-grey-new.7}"
            }
          }
        },
        opacity: {
          value: {
            _light: "{colors.light-grey.opacity}",
            _dark: {
              base: "{colors.dark-grey.opacity}",
              _newTheme: "{colors.dark-grey-new.opacity}"
            }
          }
        },
        error: {
          value: {
            _light: "{colors.red.1}",
            _dark: "{colors.red.3}"
          }
        },
        success: {
          value: {
            _light: "{colors.green.1}",
            _dark: "{colors.green.3}"
          }
        },
        progress: {
          value: {
            _light: "{colors.orange.1}",
            _dark: "{colors.orange.3}"
          }
        }
      },

      surface: {
        primary: {
          value: {
            _light: "{colors.light-grey.1}",
            _dark: {
              base: "{colors.dark-grey.8}",
              _newTheme: "{colors.dark-grey-new.7}"
            }
          }
        },
        secondary: {
          value: {
            _light: "{colors.light-grey.2}",
            _dark: {
              base: "{colors.dark-grey.7}",
              _newTheme: "{colors.dark-grey-new.6}"
            }
          }
        },
        tertiary: {
          value: {
            _light: "{colors.light-grey.3}",
            _dark: {
              base: "{colors.dark-grey.6}",
              _newTheme: "{colors.dark-grey-new.5}"
            }
          }
        },
        contrast: {
          value: {
            _light: "{colors.light-grey.8}",
            _dark: {
              base: "{colors.dark-grey.5}",
              _newTheme: "{colors.dark-grey-new.4}"
            }
          }
        }
      },

      border: {
        primary: {
          value: {
            _light: "{colors.light-grey.3}",
            _dark: {
              base: "{colors.dark-grey.6}",
              _newTheme: "{colors.dark-grey-new.6}"
            }
          }
        },
        secondary: {
          value: {
            _light: "{colors.light-grey.6}",
            _dark: {
              base: "{colors.dark-grey.3}",
              _newTheme: "{colors.dark-grey-new.3}"
            }
          }
        },
        light: {
          value: {
            _light: "{colors.light-grey.2}",
            _dark: {
              base: "{colors.dark-grey.7}",
              _newTheme: "{colors.dark-grey-new.7}"
            }
          }
        },
        error: {
          value: {
            _light: "{colors.red.2}",
            _dark: "{colors.red.2}"
          }
        }
      },

      code: {
        strings: {
          value: {
            _light: "{colors.green.2}",
            _dark: "{colors.green.2}"
          }
        },
        types: {
          value: {
            _light: "{colors.orange.2}",
            _dark: "{colors.orange.2}"
          }
        },
        "numerical-values": {
          value: {
            _light: "{colors.pink.2}",
            _dark: "{colors.pink.2}"
          }
        },
        keywords: {
          value: {
            _light: "{colors.blue.2}",
            _dark: "{colors.blue.2}"
          }
        },
        "table+column-names": {
          value: {
            _light: "{colors.light-grey.7}",
            _dark: "{colors.dark-grey.2}"
          }
        },
        comments: {
          value: {
            _light: "{colors.light-grey.4}",
            _dark: "{colors.dark-grey.4}"
          }
        }
      },

      error: {
        default: {
          value: {
            _light: "{colors.red.2}",
            _dark: "{colors.red.3}"
          }
        },
        hover: {
          value: {
            _light: "{colors.red.3}",
            _dark: "{colors.red.2}"
          }
        },
        background: {
          value: {
            _light: "{colors.red.1}",
            _dark: "{colors.red.4}"
          }
        }
      }
    },

    spacing: {
      none: {
        value: "{spacing.0}"
      },
      xxs: {
        value: "{spacing.0.5}"
      },
      xs: {
        value: "{spacing.1}"
      },
      sm: {
        value: "{spacing.2}"
      },
      md: {
        value: "{spacing.3}"
      },
      lg: {
        value: "{spacing.4}"
      },
      xl: {
        value: "{spacing.5}"
      },
      "2xl": {
        value: "{spacing.6}"
      },
      "3xl": {
        value: "{spacing.7}"
      },
      "4xl": {
        value: "{spacing.8}"
      },
      "5xl": {
        value: "{spacing.9}"
      }
    },

    radii: {
      minimal: {
        value: "{radii.sm}"
      },
      rounded: {
        value: "{radii.md}"
      },
      full: {
        value: "{radii.lg}"
      },
      extra: {
        value: "{radii.xl}"
      }
    },

    sizes: {
      xs: {
        value: "{sizes.1}"
      },
      sm: {
        value: "{sizes.2}"
      },
      md: {
        value: "{sizes.3}"
      },
      lg: {
        value: "{sizes.4}"
      },
      xl: {
        value: "{sizes.5}"
      },
      "2xl": {
        value: "{sizes.6}"
      },
      "3xl": {
        value: "{sizes.7}"
      },
      "4xl": {
        value: "{sizes.8}"
      },
      "5xl": {
        value: "{sizes.9}"
      },
      "6xl": {
        value: "{sizes.10}"
      },
      "7xl": {
        value: "{sizes.11}"
      },
      "8xl": {
        value: "{sizes.12}"
      }
    },

    shadows: {
      primary: {
        value: {
          _light: "0px 2px 2px 0px rgba(229, 229, 231, 0.48)",
          _dark: "0px 2px 6px 0px rgba(0, 0, 0, 0.60)"
        }
      },
      secondary: {
        value: {
          _light: "0px 8px 16px 0px rgba(229, 229, 231, 0.48)",
          _dark: "0px 8px 16px 0px rgba(0, 0, 0, 0.48)"
        }
      },
      error: {
        value: {
          _light: "0px 0px 8px 0px rgba(192, 68, 56, 0.20)",
          _dark: "0px 0px 8px 0px rgba(192, 68, 56, 0.60)"
        }
      },
      border: {
        value: {
          _light: "inset 0px 0px 0px 1px rgba(235, 235, 235, 0.50)",
          _dark: "inset 0px 0px 0px 1px rgba(138, 139, 141, 0.50)"
        }
      }
    }
  },
  keyframes: {
    slideRight: {
      "100%": {
        transform: "translateX(100%)"
      }
    },
    toastHide: {
      from: {
        transform: "translateX(var(--radix-toast-swipe-end-x))"
      },
      to: {
        transform: "translateX(calc(100%))"
      }
    },
    toastShow: {
      from: {
        transform: "translateX(calc(100%))"
      },
      to: {
        transform: "translateX(0px)"
      }
    },
    rotate: {
      to: { transform: "rotate(1turn)" }
    },
    dialogSlideUp: {
      from: {
        transform: "translateY(100%)"
      },
      to: {
        transform: "translateY(0)"
      }
    },
    dialogSlideDown: {
      from: {
        transform: "translateY(0)"
      },
      to: {
        transform: "translateY(100%)"
      }
    },
    fadeIn: {
      from: {
        opacity: 0
      },
      to: {
        opacity: 1
      }
    },
    fadeOut: {
      from: {
        opacity: 1
      },
      to: {
        opacity: 0
      }
    },
    dotFlashing: {
      "0%": {
        backgroundColor: "{colors.light-grey.3}"
      },
      "80%": {
        backgroundColor: "{colors.light-grey.5}"
      },
      "100%": {
        backgroundColor: "{colors.light-grey.6}"
      }
    }
  }
};
