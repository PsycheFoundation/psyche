import { text } from "../fonts.js";
import { forest, slate } from "../colors.js";
import Select from "react-select";
import { css } from "@linaria/core";
import { c, svgFillCurrentColor } from "../utils.js";

import DownChevron from "../assets/icons/chevron-down.svg?react";
import { styled } from "@linaria/react";
import { useState } from "react";

type OptionType = { label: string; value: string };
interface SortProps<T extends OptionType> {
  selected: T;
  options: ReadonlyArray<T>;
  onChange: (value: T) => void;
  className?: string;
}

const containerStyle = css`
  border: 2px solid;
  text-transform: uppercase;

  .theme-light & {
    border-color: ${forest[700]};
  }

  .theme-dark & {
    border-color: ${forest[300]};
  }

  & > option {
    background: ${slate[200]};
  }

  min-width: calc(var(--longest-option-length) + 7ch);
  text-align: center;
`;

const controlStyle = css`
  border: 0 !important;
  border-radius: 0 !important;
  background: transparent !important;
  flex-direction: row-reverse;
  min-height: 0 !important;
`;

const indicatorsStyle = css`
  .theme-light & {
    background: ${forest[700]};
  }
  .theme-dark & {
    background: ${forest[300]};
  }
`;

const placeholderStyle = css`
  color: ${slate[500]} !important;
`;

const indicatorSeparatorStyle = css`
  display: none;
`;

const valueContainerStyle = css`
  padding-left: 4px !important;
`;

const singleValueStyle = css`
  .theme-light & {
    color: ${forest[700]} !important;
  }
  .theme-dark & {
    color: ${forest[300]} !important;
  }
`;

const menuContainerStyle = css`
  border-radius: 0 !important;
  margin-top: 2px !important;
  margin-bottom: 2px !important;
  box-shadow: none !important;
  margin-left: -2px;
`;

const menuListStyle = css`
  padding-top: 0 !important;
  padding-bottom: 0 !important;
  .theme-light & {
    background: ${slate[200]};
  }
  .theme-dark & {
    background: ${forest[600]};
  }
`;

const optionStyle = css`
  padding: 2px 8px !important;
  &[aria-selected="true"] {
    .theme-light & {
      color: ${forest[700]};
      background: ${slate[400]};
    }
    .theme-dark & {
      color: ${forest[700]};
      background: ${forest[300]};
    }
  }
`;

const focusedOptionStyle = css`
  .theme-light & {
    background: ${slate[300]};
  }
  .theme-dark & {
    background: ${forest[400]};
  }
`;

const Dropdown = styled.div`
  padding-left: 4px;
  padding-right: 6px;
  display: flex;
  align-items: center;
  justify-content: center;
  transform: scaleY(${(props) => (props.flip ? "-1" : "1")});
  color: var(--color-bg);
`;

export function Sort<T extends OptionType>({
  options,
  selected,
  onChange,
  className,
}: SortProps<T>) {
  const [isOpen, setIsOpen] = useState(false);
  return (
    <Select
      styles={{
        container: (a) => ({
          ...a,
          "--longest-option-length": `${Math.max(...options.map((o) => o.label.length))}ch`,
        }),
      }}
      className={className}
      options={options}
      onChange={(v) => onChange(v!)}
      value={selected}
      placeholder="sort by..."
      components={{
        DropdownIndicator: () => (
          <Dropdown flip={isOpen}>
            <DownChevron className={svgFillCurrentColor} />
          </Dropdown>
        ),
      }}
      classNames={{
        container: () => c(containerStyle, text["button/sm"]),
        control: () => controlStyle,
        placeholder: () => placeholderStyle,
        indicatorsContainer: () => indicatorsStyle,
        indicatorSeparator: () => indicatorSeparatorStyle,
        valueContainer: () => valueContainerStyle,
        singleValue: () => singleValueStyle,
        menu: () => menuContainerStyle,
        menuList: () => menuListStyle,
        option: ({ isFocused }) =>
          c(optionStyle, isFocused && focusedOptionStyle),
      }}
      isSearchable={false}
      onMenuOpen={() => setIsOpen(true)}
      onMenuClose={() => setIsOpen(false)}
    />
  );
}
