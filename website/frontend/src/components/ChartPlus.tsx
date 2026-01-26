interface PlusSymbolProps {
  x: number;
  y: number;
  size: number;
  stroke: string;
  thickness: number;
}

const PlusSymbol: React.FC<PlusSymbolProps> = ({
  x,
  y,
  size = 5,
  thickness = 2,
  stroke,
}) => {
  const horizontalLine = {
    x1: x - size,
    x2: x + size,
    y1: y,
    y2: y,
  };

  const verticalLine = {
    x1: x,
    x2: x,
    y1: y - size,
    y2: y + size,
  };

  return (
    <g>
      <line {...verticalLine} stroke={stroke} strokeWidth={thickness} />
      <line {...horizontalLine} stroke={stroke} strokeWidth={thickness} />
    </g>
  );
};

const SplitPlusSymbol: React.FC<
  PlusSymbolProps & {
    half: "left" | "right" | "full" | "hor";
    centerLineThickness: number;
  }
> = ({ half, x, y, size, thickness, centerLineThickness, stroke }) => {
  const horizontalLineLeft = {
    x1: x - size - centerLineThickness / 2,
    x2: x - centerLineThickness / 2,
    y1: y,
    y2: y,
  };
  const horizontalLineRight = {
    x1: x + centerLineThickness / 2,
    x2: x + size + centerLineThickness / 2,
    y1: y,
    y2: y,
  };

  const verticalLineLeft = {
    x1: x - centerLineThickness / 2,
    x2: x - centerLineThickness / 2,
    y1: y - size,
    y2: y + size,
  };
  const verticalLineRight = {
    x1: x + centerLineThickness / 2,
    x2: x + centerLineThickness / 2,
    y1: y - size,
    y2: y + size,
  };

  return (
    <g>
      {half !== "right" && (
        <>
          {half !== "hor" && (
            <line
              {...verticalLineLeft}
              stroke={stroke}
              strokeWidth={thickness}
            />
          )}
          <line
            {...horizontalLineLeft}
            stroke={stroke}
            strokeWidth={thickness}
          />
        </>
      )}

      {half !== "left" && (
        <>
          {half !== "hor" && (
            <line
              {...verticalLineRight}
              stroke={stroke}
              strokeWidth={thickness}
            />
          )}
          <line
            {...horizontalLineRight}
            stroke={stroke}
            strokeWidth={thickness}
          />
        </>
      )}
    </g>
  );
};

export const GridOfPlusSymbols: React.FC<{
  verticalLinePositions: number[];
  horizontalLinePositions: number[];
  xScale: (value: number) => number;
  yScale: (value: number) => number;
  size: number;
  thickness: number;
  centerLineThickness: number;
  stroke: string;
  splitStroke: string;
}> = ({
  stroke,
  splitStroke,
  verticalLinePositions,
  horizontalLinePositions,
  xScale,
  yScale,
  size,
  thickness,
  centerLineThickness,
}) => {
  return (
    <>
      {verticalLinePositions.slice(0, -1).map((x, i) => {
        const midX = xScale((x + verticalLinePositions[i + 1]) / 2);
        return horizontalLinePositions.map((yPos, rowIndex) => (
          <PlusSymbol
            stroke={stroke}
            key={`full-plus-${i}-${rowIndex}`}
            x={midX}
            y={yScale(yPos)}
            size={size}
            thickness={thickness}
          />
        ));
      })}

      {verticalLinePositions.map((x, i) => {
        return horizontalLinePositions.map((yPos, rowIndex) => (
          <SplitPlusSymbol
            stroke={splitStroke}
            key={`half-plus-${i}-${rowIndex}`}
            x={xScale(x)}
            y={yScale(yPos)}
            size={size}
            thickness={thickness}
            centerLineThickness={centerLineThickness}
            half={i === 0 ? "right" : "hor"}
          />
        ));
      })}
    </>
  );
};
