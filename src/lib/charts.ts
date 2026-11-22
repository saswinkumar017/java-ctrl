import * as echarts from "echarts/core";
import type { EChartsCoreOption } from "echarts/core";
import { BarChart, LineChart, PieChart } from "echarts/charts";
import {
  GridComponent,
  LegendComponent,
  MarkLineComponent,
  TooltipComponent,
} from "echarts/components";
import { CanvasRenderer } from "echarts/renderers";
import { formatBytes, formatNumber, formatTime } from "./format";

echarts.use([LineChart, BarChart, PieChart, GridComponent, TooltipComponent, LegendComponent, MarkLineComponent, CanvasRenderer]);

export { type EChartsCoreOption };

export const palette = {
  cyan: "#34e0c4",
  blue: "#5aa9ff",
  violet: "#a68bff",
  amber: "#f5b955",
  red: "#ff6b7a",
  green: "#66d38c",
  pink: "#f780b9",
  slate: "#5f7590",
};

const axisLine = { lineStyle: { color: "#243244" } };
const splitLine = { lineStyle: { color: "rgba(90,120,150,0.13)", type: "dashed" as const } };
const labelStyle = { color: "#7d90a6", fontSize: 11 };

export class ChartHost {
  private chart: echarts.ECharts;
  private observer: ResizeObserver;

  constructor(element: HTMLElement) {
    this.chart = echarts.init(element, undefined, { renderer: "canvas" });
    this.observer = new ResizeObserver(() => this.chart.resize());
    this.observer.observe(element);
  }

  setOption(option: EChartsCoreOption, notMerge = true): void {
    this.chart.setOption(option, notMerge);
  }

  resize(): void {
    this.chart.resize();
  }

  dispose(): void {
    this.observer.disconnect();
    this.chart.dispose();
  }
}

export interface SeriesDef {
  name: string;
  color: string;
  data: [number, number][];
  area?: boolean;
  smooth?: boolean;
  yAxisIndex?: number;
  step?: "start" | "middle" | "end";
  dashed?: boolean;
}

export interface TimeSeriesOptions {
  valueFormat?: (value: number) => string;
  yName?: string;
  legend?: boolean;
  max?: number;
  min?: number;
  gridLeft?: number;
  gridRight?: number;
  showSymbols?: boolean;
}

function tooltipFormatter(
  params: unknown,
  valueFormat: (value: number) => string,
): string {
  const items = params as Array<{ axisValue: number; marker: string; seriesName: string; value: [number, number] | number }>;
  if (!Array.isArray(items) || items.length === 0) return "";
  const rows = items
    .map((item) => {
      const data = Array.isArray(item.value) ? item.value[1] : item.value;
      return `<div class="chart-tip-row"><span>${item.marker} ${item.seriesName}</span><b>${valueFormat(Number(data))}</b></div>`;
    })
    .join("");
  return `<div class="chart-tip"><div class="chart-tip-time">${formatTime(Number(items[0]?.axisValue))}</div>${rows}</div>`;
}

export function timeSeriesOption(series: SeriesDef[], options: TimeSeriesOptions = {}): EChartsCoreOption {
  const valueFormat = options.valueFormat ?? formatBytes;
  const axes = new Set(series.map((item) => item.yAxisIndex ?? 0));
  const yAxis = Array.from(axes).map((index) => ({
    type: "value",
    name: index === 0 ? (options.yName ?? "") : "%",
    nameTextStyle: labelStyle,
    min: index === 0 ? options.min ?? 0 : 0,
    max: index === 0 ? options.max : 100,
    axisLabel: {
      color: "#7d90a6",
      fontSize: 10,
      formatter: (value: number) => (index === 0 ? valueFormat(value) : `${Math.round(value)}%`),
    },
    axisLine: { show: false },
    axisTick: { show: false },
    splitLine: index === 0 ? splitLine : { show: false },
  }));

  return {
    animation: false,
    backgroundColor: "transparent",
    color: series.map((item) => item.color),
    grid: {
      left: options.gridLeft ?? 58,
      right: options.gridRight ?? 22,
      top: options.legend === false ? 18 : 36,
      bottom: 26,
      containLabel: false,
    },
    legend: options.legend === false
      ? undefined
      : {
          show: true,
          top: 0,
          right: 0,
          itemWidth: 9,
          itemHeight: 9,
          icon: "roundRect",
          textStyle: labelStyle,
        },
    tooltip: {
      trigger: "axis",
      backgroundColor: "rgba(11,18,27,0.96)",
      borderColor: "#243244",
      borderWidth: 1,
      textStyle: { color: "#dce8f5", fontSize: 11 },
      axisPointer: { type: "line", lineStyle: { color: "#3b5168" } },
      formatter: (params: unknown) => tooltipFormatter(params, valueFormat),
    },
    xAxis: {
      type: "time",
      axisLine,
      axisTick: { show: false },
      axisLabel: { ...labelStyle, formatter: (value: number) => formatTime(value) },
      splitLine: { show: false },
    },
    yAxis,
    series: series.map((item) => ({
      name: item.name,
      type: "line",
      showSymbol: options.showSymbols ?? false,
      smooth: item.smooth ?? false,
      step: item.step,
      yAxisIndex: item.yAxisIndex ?? 0,
      lineStyle: { width: 2, color: item.color, type: item.dashed ? "dashed" : "solid" },
      itemStyle: { color: item.color },
      areaStyle: item.area
        ? { color: new echarts.graphic.LinearGradient(0, 0, 0, 1, [
            { offset: 0, color: `${item.color}44` },
            { offset: 1, color: `${item.color}00` },
          ]) }
        : undefined,
      data: item.data,
    })),
  } as EChartsCoreOption;
}

export interface BarItem {
  name: string;
  value: number;
  color?: string;
}

export function horizontalBarOption(
  items: BarItem[],
  options: { valueFormat?: (value: number) => string; max?: number; height?: number } = {},
): EChartsCoreOption {
  const valueFormat = options.valueFormat ?? formatBytes;
  const rows = [...items].reverse();
  return {
    animation: false,
    backgroundColor: "transparent",
    grid: { left: 8, right: 74, top: 8, bottom: 4, containLabel: true },
    tooltip: {
      trigger: "item",
      backgroundColor: "rgba(11,18,27,0.96)",
      borderColor: "#243244",
      textStyle: { color: "#dce8f5", fontSize: 11 },
      formatter: (params: unknown) => {
        const item = params as { name: string; value: number };
        return `${item.name}<br/><b>${valueFormat(item.value)}</b>`;
      },
    },
    xAxis: {
      type: "value",
      max: options.max,
      axisLabel: { color: "#7d90a6", fontSize: 10, formatter: (value: number) => valueFormat(value) },
      axisLine: { show: false },
      axisTick: { show: false },
      splitLine: splitLine,
    },
    yAxis: {
      type: "category",
      data: rows.map((item) => item.name),
      axisLabel: { color: "#a8b9cc", fontSize: 11, width: 150, overflow: "truncate" },
      axisLine: { show: false },
      axisTick: { show: false },
    },
    series: [
      {
        type: "bar",
        barMaxWidth: 16,
        itemStyle: {
          borderRadius: [0, 4, 4, 0],
          color: (params: { dataIndex: number }) => rows[params.dataIndex]?.color ?? palette.blue,
        },
        label: {
          show: true,
          position: "right",
          color: "#9fb2c7",
          fontSize: 10,
          formatter: (params: { value: number }) => valueFormat(params.value),
        },
        data: rows.map((item) => item.value),
      },
    ],
  } as EChartsCoreOption;
}

export function donutOption(
  items: Array<{ name: string; value: number }>,
  colors: string[],
): EChartsCoreOption {
  return {
    animation: false,
    backgroundColor: "transparent",
    color: colors,
    tooltip: {
      trigger: "item",
      backgroundColor: "rgba(11,18,27,0.96)",
      borderColor: "#243244",
      textStyle: { color: "#dce8f5", fontSize: 11 },
      formatter: (params: unknown) => {
        const item = params as { name: string; value: number; percent: number };
        return `${item.name}<br/><b>${formatNumber(item.value)} (${item.percent}%)</b>`;
      },
    },
    legend: {
      orient: "vertical",
      right: 4,
      top: "center",
      itemWidth: 9,
      itemHeight: 9,
      icon: "roundRect",
      textStyle: labelStyle,
    },
    series: [
      {
        type: "pie",
        radius: ["54%", "78%"],
        center: ["34%", "50%"],
        avoidLabelOverlap: false,
        itemStyle: { borderColor: "#0d141d", borderWidth: 2, borderRadius: 4 },
        label: { show: false },
        emphasis: { scaleSize: 4 },
        data: items,
      },
    ],
  } as EChartsCoreOption;
}

export function gaugeOption(value: number, max: number, color = palette.cyan): EChartsCoreOption {
  const percentage = max > 0 ? Math.min(100, (value / max) * 100) : 0;
  return {
    animation: false,
    backgroundColor: "transparent",
    series: [
      {
        type: "pie",
        radius: ["72%", "92%"],
        center: ["50%", "50%"],
        startAngle: 220,
        endAngle: -40,
        silent: true,
        label: { show: false },
        data: [
          { value: percentage, itemStyle: { color, borderRadius: 6 } },
          { value: 100 - percentage, itemStyle: { color: "#1a2635" } },
        ],
      },
    ],
    graphic: [
      {
        type: "text",
        left: "center",
        top: "46%",
        style: { text: `${Math.round(percentage)}%`, fill: "#e5eef8", fontSize: 22, fontWeight: 600 },
      },
      {
        type: "text",
        left: "center",
        top: "66%",
        style: { text: formatBytes(value), fill: "#7d90a6", fontSize: 11 },
      },
    ],
  } as EChartsCoreOption;
}

export function emptyOption(): EChartsCoreOption {
  return { series: [] } as EChartsCoreOption;
}
