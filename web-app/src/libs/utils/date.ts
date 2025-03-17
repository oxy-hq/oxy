import dayjs from "dayjs";
import duration from "dayjs/plugin/duration";
import relativeTime from "dayjs/plugin/relativeTime";

dayjs.extend(relativeTime);
dayjs.extend(duration);

/**
 * Checks if a given string is a valid date.
 * @param dateStr - The string to be checked.
 * @returns boolean - True if the string is a valid date, false otherwise.
 */
export function isDate(date: string) {
  return (
    new Date(date).toString() !== "Invalid Date" && !isNaN(Date.parse(date))
  );
}

export function formatDate(date: Date, format: string) {
  return dayjs(date).format(format);
}

export function formatDateToHumanReadable(dateString: string): string {
  const currentDate = dayjs();
  const parsedDate = dayjs(dateString);

  // Calculate the difference in milliseconds between the current date and the provided date
  const diffInMilliseconds = currentDate.diff(parsedDate);

  // Format the date difference to a human-readable string
  return dayjs.duration(diffInMilliseconds).humanize();
}

export function formatStartingDate(date?: Date) {
  if (!date) {
    return "";
  }

  const today = dayjs().format("DD/MM/YYYY");
  if (today === dayjs(date).format("DD/MM/YYYY")) {
    return "Today";
  }

  const currentYear = dayjs().year();
  const yearOfDate = dayjs(date).year();

  if (currentYear !== yearOfDate) {
    return dayjs(date).format("MMMM D, YYYY");
  }

  return dayjs(date).format("dddd, MMMM D");
}
