import { styled } from "@linaria/react";
import { text } from "../fonts.js";
import { forest } from "../colors.js";

interface RadioSelectBarProps<T extends string> {
  selected?: T;
  options: ReadonlyArray<{ label: string; value: T }>;
  onChange: (value: T) => void;
}

const RadioSelectLabel = styled.label`
  border: 2px solid var(--color-fg);
  border-left-width: 1px;
  border-right-width: 1px;
  &:last-of-type {
    border-right-width: 2px;
  }
  &:first-of-type {
    border-left-width: 2px;
  }
  .theme-dark & {
    border-color: ${forest[300]};
    color: ${forest[300]};
  }
  padding: 2px 4px;

  text-transform: uppercase;
  cursor: pointer;

  & > input {
    display: none;
  }

  &:has(input:checked) {
    background: var(--color-fg);
    color: var(--color-bg);

    .theme-dark & {
      background: ${forest[300]};
    }
  }
`;

export function RadioSelectBar<T extends string>({
  options,
  selected,
  onChange,
}: RadioSelectBarProps<T>) {
  return (
    <div>
      {options.map(({ label, value }) => {
        const checked = value === selected;
        return (
          <RadioSelectLabel
            key={`${label}-${value}`}
            htmlFor={value}
            className={text["button/sm"]}
          >
            {label}
            <input
              type="radio"
              name="radio"
              value={value}
              id={value}
              checked={checked}
              onChange={() => onChange(value)}
            />
          </RadioSelectLabel>
        );
      })}
    </div>
  );
}
