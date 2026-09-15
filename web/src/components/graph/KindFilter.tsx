import { ToggleButton, ToggleButtonGroup } from "@mui/material";

export function KindFilter({
  kinds,
  value,
  onChange,
}: {
  kinds: string[];
  value: string;
  onChange: (value: string) => void;
}) {
  return (
    <ToggleButtonGroup
      exclusive
      size="small"
      value={value}
      onChange={(_, v) => v && onChange(v)}
      sx={{
        bgcolor: "#fff",
        borderRadius: 3,
        border: "1px solid #E4E9F0",
        overflow: "auto",
      }}
    >
      <ToggleButton value="all">全部</ToggleButton>
      {kinds.map((k) => (
        <ToggleButton key={k} value={k}>
          {k}
        </ToggleButton>
      ))}
    </ToggleButtonGroup>
  );
}
