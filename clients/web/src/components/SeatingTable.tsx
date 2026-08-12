import type { SeatAssignment, Student } from "../api/types";
import type { Translate } from "../i18n/messages";

type SeatingTableProps = {
  assignments: SeatAssignment[];
  students: Student[];
  t: Translate;
  onAssign: (seatId: string, studentId: string | null) => void;
};

/**
 * Table view of the same draft (D2, G-2): precision edits and the keyboard /
 * screen-reader path. Every change dispatches the same Rust editing commands
 * as the canvas; duplicates are rejected by the editor.
 */
export function SeatingTable({
  assignments,
  students,
  t,
  onAssign,
}: SeatingTableProps) {
  return (
    <div className="seating-table-wrap">
      <table className="seating-table">
        <caption className="sr-only">{t("canvas.tableCaption")}</caption>
        <thead>
          <tr>
            <th scope="col">{t("canvas.tableSeat")}</th>
            <th scope="col">{t("canvas.tableStudent")}</th>
            <th scope="col">{t("canvas.tableStatus")}</th>
          </tr>
        </thead>
        <tbody>
          {assignments.map((seat) => (
            <tr key={seat.seatId}>
              <th scope="row" className="num">
                {seat.seatId}
              </th>
              <td>
                <select
                  aria-label={t("canvas.tableStudentFor", { seat: seat.seatId })}
                  value={seat.student?.id ?? ""}
                  disabled={seat.locked}
                  onChange={(event) =>
                    onAssign(seat.seatId, event.target.value || null)
                  }
                >
                  <option value="">{t("canvas.tableEmpty")}</option>
                  {students.map((student) => (
                    <option key={student.id} value={student.id}>
                      {student.name} · {student.id}
                    </option>
                  ))}
                </select>
              </td>
              <td>
                {seat.locked ? (
                  <span className="chip purple">{t("canvas.locked")}</span>
                ) : (
                  <span className="chip green">{t("canvas.tableNormal")}</span>
                )}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
