import { textStyles } from "./text-styles";

export const theme = {
  breakpoints: {
    sm: "640px",
    md: "768px",
    lg: "1024px",
    xl: "1280px",
    "2xl": "1536px",
  },
  textStyles,
  tokens: {
    fonts: {
      Inter: {
        value: "var(--font-family-inter)",
      },
      GeistMono: {
        value: "var(--font-family-geist-mono)",
      },
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
        opacity: { value: "rgba(248,248,248,0.7)" },
      },

      // new
      grey: {
        "50": { value: "#F9FAFB" },
        "100": { value: "#F3F4F6" },
        "200": { value: "#E5E7EB" },
        "300": { value: "#D1D5DB" },
        "400": { value: "#9CA3AF" },
        "500": { value: "#6B7280" },
        "600": { value: "#4B5563" },
        "700": { value: "#374151" },
        "800": { value: "#1F2937" },
        "900": { value: "#111827" },
        "950": { value: "#030712" },
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
        opacity: { value: "rgba(0,0,0,0.6)" },

        // new
        "50": { value: "#030712" },
        "100": { value: "#111827" },
        "200": { value: "#1F2937" },
        "300": { value: "#374151" },
        "400": { value: "#4B5563" },
        "500": { value: "#6B7280" },
        "600": { value: "#9CA3AF" },
        "700": { value: "#D1D5DB" },
        "800": { value: "#E5E7EB" },
        "900": { value: "#F3F4F6" },
        "950": { value: "#F9FAFB" },
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
        opacity: { value: "rgba(84,87,95,0.3)" },
      },

      // new
      neutral: {
        "50": { value: "#FAFAFA" },
        "100": { value: "#F5F5F5" },
        "200": { value: "#E5E5E5" },
        "300": { value: "#D4D4D4" },
        "400": { value: "#A3A3A3" },
        "500": { value: "#737373" },
        "600": { value: "#525252" },
        "700": { value: "#404040" },
        "800": { value: "#262626" },
        "900": { value: "#171717" },
        "950": { value: "#0A0A0A" },
      },

      // new
      "dark-neutral": {
        "50": { value: "#0A0A0A" },
        "100": { value: "#171717" },
        "200": { value: "#262626" },
        "300": { value: "#404040" },
        "400": { value: "#525252" },
        "500": { value: "#737373" },
        "600": { value: "#A3A3A3" },
        "700": { value: "#D4D4D4" },
        "800": { value: "#E5E5E5" },
        "900": { value: "#F5F5F5" },
        "950": { value: "#FAFAFA" },
      },

      // new
      zinc: {
        "50": { value: "##FAFAFA" },
        "100": { value: "#F4F4F5" },
        "200": { value: "#E4E4E7" },
        "300": { value: "#D4D4D8" },
        "400": { value: "#A1A1AA" },
        "500": { value: "#71717A" },
        "600": { value: "#52525B" },
        "700": { value: "#3F3F46" },
        "800": { value: "#27272A" },
        "900": { value: "#18181B" },
        "950": { value: "#09090B" },
      },

      // new
      "dark-zinc": {
        "50": { value: "#09090B" },
        "100": { value: "#18181B" },
        "200": { value: "#27272A" },
        "300": { value: "#3F3F46" },
        "400": { value: "#52525B" },
        "500": { value: "#71717A" },
        "600": { value: "#A1A1AA" },
        "700": { value: "#D4D4D8" },
        "800": { value: "#E4E4E7" },
        "900": { value: "#F4F4F5" },
        "950": { value: "#FAFAFA" },
      },
      red: {
        "1": { value: "#F2A6A6" },
        "2": { value: "#C04438" },
        "3": { value: "#812A22" },
        "4": { value: "#591D17" },

        // new
        "100": { value: "#FFF5F5" },
        "200": { value: "#FFE2E0" },
        "300": { value: "#FFC7C2" },
        "400": { value: "#FFAFA3" },
        "500": { value: "#F24822" },
        "600": { value: "#DC3412" },
        "700": { value: "#BD2915" },
        "800": { value: "#9F1F18" },
        "900": { value: "#771208" },
        "950": { value: "#660E0B" },
      },

      // new
      "dark-red": {
        "100": { value: "#660E0B" },
        "200": { value: "#771208" },
        "300": { value: "#9F1F18" },
        "400": { value: "#BD2915" },
        "500": { value: "#DC3412" },
        "600": { value: "#F24822" },
        "700": { value: "#FFAFA3" },
        "800": { value: "#FFC7C2" },
        "900": { value: "#FFE2E0" },
        "950": { value: "#FFF5F5" },
      },

      graph: {
        "1": { value: "#6065A8" },
        "2": { value: "#515695" },
        "3": { value: "#484C88" },
        "4": { value: "#3E427B" },
        "5": { value: "#363A6F" },
        "6": { value: "#2C3062" },
        "7": { value: "#252858" },
      },
      green: {
        "1": { value: "#E8F5F1" },
        "2": { value: "#1E9B70" },
        "3": { value: "#203030" },
      },
      orange: {
        "1": { value: "#FDF2EB" },
        "2": { value: "#ED8132" },
        "3": { value: "#3F2D24" },
      },

      pink: {
        "2": {
          value: "#E2237E",
        },
      },
      blue: {
        "2": {
          value: "#5186EE",
        },
      },
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
      "9": { value: "40px" },

      // new
      sizeXXXS: { value: "2px" },
      sizeXXS: { value: "4px" },
      sizeXS: { value: "8px" },
      sizeSM: { value: "12px" },
      size: { value: "16px" },
      sizeMS: { value: "16px" },
      sizeMD: { value: "20px" },
      sizeLG: { value: "24px" },
      sizeXL: { value: "32px" },
      sizeXXL: { value: "48px" },
    },
    radii: {
      sm: { value: "4px" },
      md: { value: "8px" },
      lg: { value: "12px" },
      xl: { value: "16px" },
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
      13: { value: "56px" },
    },
  },
  semanticTokens: {
    colors: {
      text: {
        primary: {
          value: {
            _light: "{colors.light-grey.7}",
            _dark: {
              base: "{colors.dark-grey.2}",
              _newTheme: "{colors.dark-grey-new.1}",
            },
          },
        },
        secondary: {
          value: {
            _light: "{colors.light-grey.4}",
            _dark: {
              base: "{colors.dark-grey.4}",
              _newTheme: "{colors.dark-grey-new.3}",
            },
          },
        },
        light: {
          value: {
            _light: "{colors.light-grey.6}",
            _dark: {
              base: "{colors.dark-grey.3}",
              _newTheme: "{colors.dark-grey-new.2}",
            },
          },
        },
        contrast: {
          value: {
            _light: "{colors.light-grey.1}",
            _dark: {
              base: "{colors.dark-grey.2}",
              _newTheme: "{colors.dark-grey-new.1}",
            },
          },
        },
        "less-contrast": {
          value: {
            _light: "{colors.light-grey.5}",
            _dark: {
              base: "{colors.dark-grey.3}",
              _newTheme: "{colors.dark-grey-new.3}",
            },
          },
        },
        disabled: {
          value: {
            _light: "{colors.light-grey.3}",
            _dark: {
              base: "{colors.dark-grey.3}",
              _newTheme: "{colors.dark-grey-new.4}",
            },
          },
        },
        error: {
          value: {
            _light: "{colors.red.2}",
            _dark: "{colors.red.2}",
          },
        },
        success: {
          value: {
            _light: "{colors.green.2}",
            _dark: "{colors.green.2}",
          },
        },
        progress: {
          value: {
            _light: "{colors.orange.2}",
            _dark: "{colors.orange.2}",
          },
        },
      },

      background: {
        primary: {
          value: {
            _light: "{colors.light-grey.1}",
            _dark: {
              base: "{colors.dark-grey.8}",
              _newTheme: "{colors.dark-grey-new.8}",
            },
          },
        },
        secondary: {
          value: {
            _light: "{colors.light-grey.2}",
            _dark: {
              base: "{colors.dark-grey.7}",
              _newTheme: "{colors.dark-grey-new.7}",
            },
          },
        },
        opacity: {
          value: {
            _light: "{colors.light-grey.opacity}",
            _dark: {
              base: "{colors.dark-grey.opacity}",
              _newTheme: "{colors.dark-grey-new.opacity}",
            },
          },
        },
        error: {
          value: {
            _light: "{colors.red.1}",
            _dark: "{colors.red.3}",
          },
        },
        success: {
          value: {
            _light: "{colors.green.1}",
            _dark: "{colors.green.3}",
          },
        },
        progress: {
          value: {
            _light: "{colors.orange.1}",
            _dark: "{colors.orange.3}",
          },
        },
      },

      surface: {
        primary: {
          value: {
            _light: "{colors.light-grey.1}",
            _dark: {
              base: "{colors.dark-grey.8}",
              _newTheme: "{colors.dark-grey-new.7}",
            },
          },
        },
        secondary: {
          value: {
            _light: "{colors.light-grey.2}",
            _dark: {
              base: "{colors.dark-grey.7}",
              _newTheme: "{colors.dark-grey-new.6}",
            },
          },
        },
        tertiary: {
          value: {
            _light: "{colors.light-grey.3}",
            _dark: {
              base: "{colors.dark-grey.6}",
              _newTheme: "{colors.dark-grey-new.5}",
            },
          },
        },
        contrast: {
          value: {
            _light: "{colors.light-grey.8}",
            _dark: {
              base: "{colors.dark-grey.5}",
              _newTheme: "{colors.dark-grey-new.4}",
            },
          },
        },
      },

      border: {
        primary: {
          value: {
            _light: "{colors.light-grey.3}",
            _dark: {
              base: "{colors.dark-grey.6}",
              _newTheme: "{colors.dark-grey-new.6}",
            },
          },
        },
        secondary: {
          value: {
            _light: "{colors.light-grey.6}",
            _dark: {
              base: "{colors.dark-grey.3}",
              _newTheme: "{colors.dark-grey-new.3}",
            },
          },
        },
        light: {
          value: {
            _light: "{colors.light-grey.2}",
            _dark: {
              base: "{colors.dark-grey.7}",
              _newTheme: "{colors.dark-grey-new.7}",
            },
          },
        },
        error: {
          value: {
            _light: "{colors.red.2}",
            _dark: "{colors.red.2}",
          },
        },
      },

      code: {
        strings: {
          value: {
            _light: "{colors.green.2}",
            _dark: "{colors.green.2}",
          },
        },
        types: {
          value: {
            _light: "{colors.orange.2}",
            _dark: "{colors.orange.2}",
          },
        },
        "numerical-values": {
          value: {
            _light: "{colors.pink.2}",
            _dark: "{colors.pink.2}",
          },
        },
        keywords: {
          value: {
            _light: "{colors.blue.2}",
            _dark: "{colors.blue.2}",
          },
        },
        "table+column-names": {
          value: {
            _light: "{colors.light-grey.7}",
            _dark: "{colors.dark-grey.2}",
          },
        },
        comments: {
          value: {
            _light: "{colors.light-grey.4}",
            _dark: "{colors.dark-grey.4}",
          },
        },
      },

      error: {
        default: {
          value: {
            _light: "{colors.red.2}",
            _dark: "{colors.red.3}",
          },
        },
        hover: {
          value: {
            _light: "{colors.red.3}",
            _dark: "{colors.red.2}",
          },
        },
        background: {
          value: {
            _light: "{colors.red.1}",
            _dark: "{colors.red.4}",
          },
        },
      },
      // new
      base: {
        grey: {
          50: {
            value: {
              _light: "{colors.grey.50}",
              _dark: "{colors.dark-grey.50}",
            },
          },
          100: {
            value: {
              _light: "{colors.grey.100}",
              _dark: "{colors.dark-grey.100}",
            },
          },
          200: {
            value: {
              _light: "{colors.grey.200}",
              _dark: "{colors.dark-grey.200}",
            },
          },
          300: {
            value: {
              _light: "{colors.grey.300}",
              _dark: "{colors.dark-grey.300}",
            },
          },
          400: {
            value: {
              _light: "{colors.grey.400}",
              _dark: "{colors.dark-grey.400}",
            },
          },
          500: {
            value: {
              _light: "{colors.grey.500}",
              _dark: "{colors.dark-grey.500}",
            },
          },
          600: {
            value: {
              _light: "{colors.grey.600}",
              _dark: "{colors.dark-grey.600}",
            },
          },
          700: {
            value: {
              _light: "{colors.grey.700}",
              _dark: "{colors.dark-grey.700}",
            },
          },
          800: {
            value: {
              _light: "{colors.grey.800}",
              _dark: "{colors.dark-grey.800}",
            },
          },
          900: {
            value: {
              _light: "{colors.grey.900}",
              _dark: "{colors.dark-grey.900}",
            },
          },
          950: {
            value: {
              _light: "{colors.grey.950}",
              _dark: "{colors.dark-grey.950}",
            },
          },
        },
        neutral: {
          50: {
            value: {
              _light: "{colors.neutral.50}",
              _dark: "{colors.dark-neutral.50}",
            },
          },
          100: {
            value: {
              _light: "{colors.neutral.100}",
              _dark: "{colors.dark-neutral.100}",
            },
          },
          200: {
            value: {
              _light: "{colors.neutral.200}",
              _dark: "{colors.dark-neutral.200}",
            },
          },
          300: {
            value: {
              _light: "{colors.neutral.300}",
              _dark: "{colors.dark-neutral.300}",
            },
          },
          400: {
            value: {
              _light: "{colors.neutral.400}",
              _dark: "{colors.dark-neutral.400}",
            },
          },
          500: {
            value: {
              _light: "{colors.neutral.500}",
              _dark: "{colors.dark-neutral.500}",
            },
          },
          600: {
            value: {
              _light: "{colors.neutral.600}",
              _dark: "{colors.dark-neutral.600}",
            },
          },
          700: {
            value: {
              _light: "{colors.neutral.700}",
              _dark: "{colors.dark-neutral.700}",
            },
          },
          800: {
            value: {
              _light: "{colors.neutral.800}",
              _dark: "{colors.dark-neutral.800}",
            },
          },
          900: {
            value: {
              _light: "{colors.neutral.900}",
              _dark: "{colors.dark-neutral.900}",
            },
          },
          950: {
            value: {
              _light: "{colors.neutral.950}",
              _dark: "{colors.dark-neutral.950}",
            },
          },
        },
        zinc: {
          50: {
            value: {
              _light: "{colors.zinc.50}",
              _dark: "{colors.dark-zinc.50}",
            },
          },
          100: {
            value: {
              _light: "{colors.zinc.100}",
              _dark: "{colors.dark-zinc.100}",
            },
          },
          200: {
            value: {
              _light: "{colors.zinc.200}",
              _dark: "{colors.dark-zinc.200}",
            },
          },
          300: {
            value: {
              _light: "{colors.zinc.300}",
              _dark: "{colors.dark-zinc.300}",
            },
          },
          400: {
            value: {
              _light: "{colors.zinc.400}",
              _dark: "{colors.dark-zinc.400}",
            },
          },
          500: {
            value: {
              _light: "{colors.zinc.500}",
              _dark: "{colors.dark-zinc.500}",
            },
          },
          600: {
            value: {
              _light: "{colors.zinc.600}",
              _dark: "{colors.dark-zinc.600}",
            },
          },
          700: {
            value: {
              _light: "{colors.zinc.700}",
              _dark: "{colors.dark-zinc.700}",
            },
          },
          800: {
            value: {
              _light: "{colors.zinc.800}",
              _dark: "{colors.dark-zinc.800}",
            },
          },
          900: {
            value: {
              _light: "{colors.zinc.900}",
              _dark: "{colors.dark-zinc.900}",
            },
          },
          950: {
            value: {
              _light: "{colors.zinc.950}",
              _dark: "{colors.dark-zinc.950}",
            },
          },
        },
        red: {
          100: {
            value: {
              _light: "{colors.red.100}",
              _dark: "{colors.dark-red.100}",
            },
          },
          200: {
            value: {
              _light: "{colors.red.200}",
              _dark: "{colors.dark-red.200}",
            },
          },
          300: {
            value: {
              _light: "{colors.red.300}",
              _dark: "{colors.dark-red.300}",
            },
          },
          400: {
            value: {
              _light: "{colors.red.400}",
              _dark: "{colors.dark-red.400}",
            },
          },
          500: {
            value: {
              _light: "{colors.red.500}",
              _dark: "{colors.dark-red.500}",
            },
          },
          600: {
            value: {
              _light: "{colors.red.600}",
              _dark: "{colors.dark-red.600}",
            },
          },
          700: {
            value: {
              _light: "{colors.red.700}",
              _dark: "{colors.dark-red.700}",
            },
          },
          800: {
            value: {
              _light: "{colors.red.800}",
              _dark: "{colors.dark-red.800}",
            },
          },
          900: {
            value: {
              _light: "{colors.red.900}",
              _dark: "{colors.dark-red.900}",
            },
          },
          950: {
            value: {
              _light: "{colors.red.950}",
              _dark: "{colors.dark-red.950}",
            },
          },
        },
      },

      // new
      neutral: {
        colorWhite: {
          value: {
            _light: "#FFFFFF",
            _dark: "#FFFFFF",
          },
        },
        colorBgBase: {
          value: {
            _light: "#F5F5F5",
            _dark: "{colors.base.neutral.50}",
          },
        },
        colorBgSecondary: {
          value: {
            _light: "#F8F8F8",
            _dark: "{colors.base.neutral.100}",
          },
        },
        colorBgTertiary: {
          value: {
            _light: "#FBFBFB",
            _dark: "{colors.base.neutral.300}",
          },
        },
        colorTextBase: {
          value: {
            _light: "#000000",
            _dark: "#FFFFFF",
          },
        },
        transparent: {
          value: {
            _light: "transparent",
            _dark: "transparent",
          },
        },
        icon: {
          colorIcon: {
            value: {
              _light: "{colors.neutral.text.colorTextTertiary}",
              _dark: "{colors.neutral.text.colorTextTertiary}",
            },
          },
          colorIconHover: {
            value: {
              _light: "{colors.neutral.text.colorText}",
              _dark: "{colors.neutral.text.colorText}",
            },
          },
          colorIconFocused: {
            value: {
              _light: "{colors.neutral.text.colorText}",
              _dark: "{colors.neutral.text.colorText}",
            },
          },
        },
        text: {
          colorText: {
            value: {
              _light: "rgba(0, 0, 0, 0.9)",
              _dark: "rgba(255, 255, 255, 0.9)",
            },
          },
          colorTextSecondary: {
            value: {
              _light: "rgba(0, 0, 0, 0.5)",
              _dark: "rgba(255, 255, 255, 0.7)",
            },
          },
          colorTextTertiary: {
            value: {
              _light: "rgba(0, 0, 0, 0.45)",
              _dark: "rgba(255, 255, 255, 0.45)",
            },
          },
          colorTextQuaternary: {
            value: {
              _light: "rgba(0, 0, 0, 0.25)",
              _dark: "rgba(255, 255, 255, 0.25)",
            },
          },
          colorTextLightSolid: {
            value: "#FFFFFF",
          },
          colorTextHeading: {
            value: {
              _light: "{colors.neutral.text.colorText}",
              _dark: "{colors.neutral.text.colorText}",
            },
          },
          colorTextLabel: {
            value: {
              _light: "{colors.neutral.text.colorTextSecondary}",
              _dark: "{colors.neutral.text.colorTextSecondary}",
            },
          },
          colorTextDescription: {
            value: {
              _light: "{colors.neutral.text.colorTextTertiary}",
              _dark: "{colors.neutral.text.colorTextTertiary}",
            },
          },
          colorTextDisabled: {
            value: {
              _light: "{colors.neutral.text.colorTextQuaternary}",
              _dark: "{colors.neutral.text.colorTextQuaternary}",
            },
          },
          colorTextPlaceholder: {
            value: {
              _light: "{colors.neutral.text.colorTextQuaternary}",
              _dark: "{colors.neutral.text.colorTextQuaternary}",
            },
          },
          solidTextColor: {
            value: {
              _light: "#FFFFFF",
              _dark: "#000000",
            },
          },
          colorCodeColor: {
            value: {
              _light: "#ABB1BF",
              _dark: "#ABB1BF",
            },
          },
          colorTextHighlight: {
            value: {
              _light: "rgba(13, 153, 255, 0.4)",
              _dark: "rgba(13, 153, 255, 0.4)",
            },
          },
        },
        bg: {
          colorBg: {
            value: {
              _light: "{colors.neutral.colorWhite}",
              _dark: "{colors.neutral.colorWhite}",
            },
          },
          colorBgContainer: {
            value: {
              _light: "{colors.neutral.colorWhite}",
              _dark: "{colors.neutral.colorWhite}",
            },
          },
          colorBgElevated: {
            value: {
              _light: "{colors.neutral.colorWhite}",
              _dark: "{colors.neutral.colorWhite}",
            },
          },
          colorBgSolid: {
            value: {
              _light: "{colors.base.neutral.950}",
              _dark: "{colors.base.neutral.950}",
            },
          },
          colorBgActive: {
            value: {
              _light: "{colors.neutral.fill.colorFillTertiary}",
              _dark: "{colors.neutral.fill.colorFillTertiary}",
            },
          },
          colorBgHover: {
            value: {
              _light: "{colors.neutral.fill.colorFillQuaternary}",
              _dark: "{colors.neutral.fill.colorFillQuaternary}",
            },
          },
          colorBgLayout: {
            value: {
              _light: "{colors.neutral.colorBgSecondary}",
              _dark: "{colors.neutral.colorBgSecondary}",
            },
          },
          colorBgContainerDisabled: {
            value: {
              _light: "{colors.neutral.fill.colorFillQuaternary}",
              _dark: "{colors.neutral.fill.colorFillQuaternary}",
            },
          },
          colorBgTextHover: {
            value: {
              _light: "{colors.neutral.fill.colorFillTertiary}",
              _dark: "{colors.neutral.fill.colorFillTertiary}",
            },
          },
          colorBgEditor: {
            value: {
              _light: "{colors.neutral.colorBgSecondary}",
              _dark: "{colors.neutral.colorBgSecondary}",
            },
          },
          colorBgOverlay: { value: "rgba(0, 0, 0, 0.5)" },
        },
        border: {
          colorBorder: {
            value: {
              _light: "{colors.base.neutral.300}",
              _dark: "{colors.base.neutral.300}",
            },
          },
          colorBorderSecondary: {
            value: {
              _light: "{colors.base.neutral.200}",
              _dark: "{colors.base.neutral.200}",
            },
          },
          colorBorderTertiary: {
            value: {
              _light: "{colors.base.neutral.100}",
              _dark: "{colors.base.neutral.100}",
            },
          },
          colorSplit: {
            value: "rgba(0, 0, 0, 0.06)",
          },
        },
        fill: {
          colorFill: {
            value: {
              _light: "rgba(0, 0, 0, 0.15)",
              _dark: "rgba(255, 255, 255, 0.18)",
            },
          },
          colorFillSecondary: {
            value: {
              _light: "rgba(0, 0, 0, 0.06)",
              _dark: "rgba(255, 255, 255, 0.12)",
            },
          },

          colorFillTertiary: {
            value: {
              _light: "rgba(0, 0, 0, 0.04)",
              _dark: "rgba(255, 255, 255, 0.08)",
            },
          },
          colorFillQuaternary: {
            value: {
              _light: "rgba(0, 0, 0, 0.02)",
              _dark: "rgba(255, 255, 255, 0.04)",
            },
          },

          colorFillContent: {
            value: {
              _light: "{colors.neutral.fill.colorFillSecondary}",
              _dark: "{colors.neutral.fill.colorFillSecondary}",
            },
          },
          colorFillContentHover: {
            value: {
              _light: "{colors.neutral.fill.colorFill}",
              _dark: "{colors.neutral.fill.colorFill}",
            },
          },
        },

        dropShadow: {
          colorPrimary: {
            value: {
              _light: "rgba(229, 229, 231, 0.48)",
              _dark: "rgba(0, 0, 0, 0.60)",
            },
          },
          colorSecondary: {
            value: {
              _light: "rgba(229, 229, 231, 0.48)",
              _dark: "rgba(0, 0, 0, 0.48)",
            },
          },
        },
      },

      // new
      brand: {
        logo: {
          value: {
            _light: { value: "#000000" },
            _dark: { value: "#FFFFFF" },
          },
        },
        primary: {
          colorPrimary: {
            value: {
              _light: "{colors.base.neutral.950}",
              _dark: "{colors.base.neutral.950}",
            },
          },
          colorPrimaryBg: {
            value: {
              _light: "{colors.base.neutral.100}",
              _dark: "{colors.base.neutral.100}",
            },
          },

          colorPrimaryBgHover: {
            value: {
              _light: "{colors.base.neutral.200}",
              _dark: "{colors.base.neutral.200}",
            },
          },

          colorPrimaryBorder: {
            value: {
              _light: "{colors.base.neutral.300}",
              _dark: "{colors.base.neutral.300}",
            },
          },

          colorPrimaryBorderHover: {
            value: {
              _light: "{colors.base.neutral.400}",
              _dark: "{colors.base.neutral.400}",
            },
          },

          colorPrimaryHover: {
            value: {
              _light: "{colors.base.neutral.700}",
              _dark: "{colors.base.neutral.700}",
            },
          },

          colorPrimaryActive: {
            value: {
              _light: "{colors.base.neutral.700}",
              _dark: "{colors.base.neutral.700}",
            },
          },

          colorPrimaryTextHover: {
            value: {
              _light: "{colors.base.neutral.500}",
              _dark: "{colors.base.neutral.500}",
            },
          },

          colorPrimaryText: {
            value: {
              _light: "{colors.base.neutral.600}",
              _dark: "{colors.base.neutral.600}",
            },
          },

          colorPrimaryTextActive: {
            value: {
              _light: "{colors.base.neutral.700}",
              _dark: "{colors.base.neutral.700}",
            },
          },
        },

        control: {
          controlItemBgActive: {
            value: {
              _light: "{colors.brand.primary.colorPrimaryBg}",
              _dark: "{colors.brand.primary.colorPrimaryBg}",
            },
          },
          controlItemBgActiveDisabled: {
            value: {
              _light: "{colors.neutral.fill.colorFill}",
              _dark: "{colors.neutral.fill.colorFill}",
            },
          },
        },
        error: {
          colorError: {
            value: {
              _light: "{colors.base.red.500}",
              _dark: "{colors.base.red.500}",
            },
          },
          colorTextDanger: {
            value: {
              _light: "{colors.base.red.600}",
              _dark: "{colors.base.red.600}",
            },
          },

          colorTextDangerHover: {
            value: {
              _light: "{colors.base.red.700}",
              _dark: "{colors.base.red.700}",
            },
          },

          colorBgDangerHover: {
            value: {
              _light: "rgba(220, 52, 18, 0.02)",
              _dark: "rgba(102, 14, 11, 0.10)",
            },
          },
        },
      },
    },

    spacing: {
      // new
      margin: {
        margin: { value: "{spacing.size}" },
        marginXS: { value: "{spacing.sizeXS}" },
        marginXXS: { value: "{spacing.sizeXXS}" },
      },
      padding: {
        paddingContentHorizontalLG: { value: "24px" },
        padding: { value: "{spacing.size}" },
        paddingXXXS: { value: "{spacing.sizeXXXS}" },
        paddingXXS: { value: "{spacing.sizeXXS}" },
        paddingXS: { value: "{spacing.sizeXS}" },
        paddingSM: { value: "{spacing.sizeSM}" },
      },
      gap: {
        gap: { value: "{spacing.size}" },
        gapXS: { value: "{spacing.sizeXS}" },
        gapXXS: { value: "{spacing.sizeXXS}" },
        gapXL: { value: "{spacing.sizeXL}" },
      },

      // old (titanium)
      none: {
        value: "{spacing.0}",
      },
      xxs: {
        value: "{spacing.0.5}",
      },
      xs: {
        value: "{spacing.1}",
      },
      sm: {
        value: "{spacing.2}",
      },
      md: {
        value: "{spacing.3}",
      },
      lg: {
        value: "{spacing.4}",
      },
      xl: {
        value: "{spacing.5}",
      },
      "2xl": {
        value: "{spacing.6}",
      },
      "3xl": {
        value: "{spacing.7}",
      },
      "4xl": {
        value: "{spacing.8}",
      },
      "5xl": {
        value: "{spacing.9}",
      },
    },

    radii: {
      // new
      borderRadiusXS: {
        value: "2px",
      },
      borderRadiusSM: {
        value: "4px",
      },
      borderRadius: {
        value: "6px",
      },
      borderRadiusMS: {
        value: "8px",
      },
      borderRadiusMD: {
        value: "10px",
      },
      borderRadiusLG: {
        value: "12px",
      },
      borderRadiusXL: {
        value: "16px",
      },
      borderRadiusXXL: {
        value: "20px",
      },

      // old (titanium)
      minimal: {
        value: "{radii.sm}",
      },
      rounded: {
        value: "{radii.md}",
      },
      full: {
        value: "{radii.lg}",
      },
      extra: {
        value: "{radii.xl}",
      },
    },

    sizes: {
      xs: {
        value: "{sizes.1}",
      },
      sm: {
        value: "{sizes.2}",
      },
      md: {
        value: "{sizes.3}",
      },
      lg: {
        value: "{sizes.4}",
      },
      xl: {
        value: "{sizes.5}",
      },
      "2xl": {
        value: "{sizes.6}",
      },
      "3xl": {
        value: "{sizes.7}",
      },
      "4xl": {
        value: "{sizes.8}",
      },
      "5xl": {
        value: "{sizes.9}",
      },
      "6xl": {
        value: "{sizes.10}",
      },
      "7xl": {
        value: "{sizes.11}",
      },
      "8xl": {
        value: "{sizes.12}",
      },
    },

    shadows: {
      primary: {
        value: {
          _light: "0px 2px 2px 0px rgba(229, 229, 231, 0.48)",
          _dark: "0px 2px 6px 0px rgba(0, 0, 0, 0.60)",
        },
      },
      secondary: {
        value: {
          _light: "0px 8px 16px 0px rgba(229, 229, 231, 0.48)",
          _dark: "0px 8px 16px 0px rgba(0, 0, 0, 0.48)",
        },
      },
      error: {
        value: {
          _light: "0px 0px 8px 0px rgba(192, 68, 56, 0.20)",
          _dark: "0px 0px 8px 0px rgba(192, 68, 56, 0.60)",
        },
      },
      border: {
        value: {
          _light: "inset 0px 0px 0px 1px rgba(235, 235, 235, 0.50)",
          _dark: "inset 0px 0px 0px 1px rgba(138, 139, 141, 0.50)",
        },
      },
    },
  },
  keyframes: {
    slideRight: {
      "100%": {
        transform: "translateX(100%)",
      },
    },
    toastHide: {
      from: {
        transform: "translateX(var(--radix-toast-swipe-end-x))",
      },
      to: {
        transform: "translateX(calc(100%))",
      },
    },
    toastShow: {
      from: {
        transform: "translateX(calc(100%))",
      },
      to: {
        transform: "translateX(0px)",
      },
    },
    rotate: {
      to: { transform: "rotate(1turn)" },
    },
    dialogSlideUp: {
      from: {
        transform: "translateY(100%)",
      },
      to: {
        transform: "translateY(0)",
      },
    },
    dialogSlideDown: {
      from: {
        transform: "translateY(0)",
      },
      to: {
        transform: "translateY(100%)",
      },
    },
    fadeIn: {
      from: {
        opacity: 0,
      },
      to: {
        opacity: 1,
      },
    },
    fadeOut: {
      from: {
        opacity: 1,
      },
      to: {
        opacity: 0,
      },
    },
    dotFlashing: {
      "0%": {
        backgroundColor: "{colors.light-grey.3}",
      },
      "80%": {
        backgroundColor: "{colors.light-grey.5}",
      },
      "100%": {
        backgroundColor: "{colors.light-grey.6}",
      },
    },
  },
};
