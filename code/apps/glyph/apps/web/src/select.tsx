import { Select } from "@base-ui/react/select";
import { ChevronsUpDown } from "lucide-react";

/** A quiet dropdown for one of a few choices, such as a pair's model. */
export function Choice<T extends string>({
  value,
  options,
  label,
  onChange,
  disabled,
}: {
  value: T;
  options: { value: T; label: string }[];
  label: string;
  onChange: (value: T) => void;
  disabled?: boolean;
}) {
  return (
    <Select.Root
      items={options}
      value={value}
      disabled={disabled}
      onValueChange={(next) => {
        if (next !== null && next !== value) onChange(next as T);
      }}
    >
      <Select.Trigger
        aria-label={label}
        title={label}
        className="flex h-7 min-w-0 items-center gap-1.5 rounded-md px-2 text-xs text-muted outline-none hover:bg-hover hover:text-fg focus-visible:ring-2 focus-visible:ring-accent data-disabled:hover:bg-transparent data-popup-open:bg-hover"
      >
        <span className="truncate">
          <Select.Value />
        </span>
        {!disabled && (
          <Select.Icon className="text-faint">
            <ChevronsUpDown size={15} strokeWidth={1.75} aria-hidden="true" />
          </Select.Icon>
        )}
      </Select.Trigger>
      <Select.Portal>
        <Select.Positioner
          className="z-50 outline-none"
          sideOffset={4}
          alignItemWithTrigger={false}
        >
          <Select.Popup className="ui-menu min-w-(--anchor-width) p-1 outline-none">
            <Select.List>
              {options.map((option) => (
                <Select.Item
                  key={option.value}
                  value={option.value}
                  className="flex cursor-default items-center rounded-md px-2 py-1.5 outline-none select-none data-highlighted:bg-highlight"
                >
                  <Select.ItemText>{option.label}</Select.ItemText>
                </Select.Item>
              ))}
            </Select.List>
          </Select.Popup>
        </Select.Positioner>
      </Select.Portal>
    </Select.Root>
  );
}

/** A number saved when you leave the field or press Enter; out-of-range values go back. */
export function NumberField({
  value,
  label,
  min,
  max,
  step,
  onChange,
  disabled,
}: {
  value: number;
  label: string;
  min: number;
  max: number;
  step: number;
  onChange: (value: number) => void;
  disabled?: boolean;
}) {
  return (
    <input
      key={value}
      type="number"
      aria-label={label}
      title={label}
      defaultValue={value}
      min={min}
      max={max}
      step={step}
      disabled={disabled}
      onBlur={(event) => {
        const next = Number(event.currentTarget.value);
        if (Number.isFinite(next) && next >= min && next <= max) {
          if (next !== value) onChange(next);
        } else {
          event.currentTarget.value = String(value);
        }
      }}
      onKeyDown={(event) => event.key === "Enter" && event.currentTarget.blur()}
      className="ui-field h-7 w-16 rounded-md px-2 text-xs tabular-nums disabled:bg-transparent"
    />
  );
}
