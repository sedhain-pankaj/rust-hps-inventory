import { escapeHtml, invoke, setBusy } from "../api.js";
import { alertModal, requestEnroll } from "../auth.js";
import { app, state, permissionLabels, setPanel, table, formatTimestamp, fingerOptions, emptyEmployee } from "../core.js";

export async function renderEmployeesPanel() {
  const employees = await invoke("list_staff", { includeInactive: true });
  if (state.employeeFormMode === "new" || state.employeeFormMode === "edit") {
    return renderEmployeeForm(employees);
  }
  setPanel(
    "Employees",
    `<button class="primary" data-new-employee>Add New Employee</button>`,
    table(
      ["Name", "ID", "Role", "Admin", "Password", "Fingerprint", "Active"],
      employees.map((employee) => ({
        clickable: true,
        attrs: `data-select-employee="${escapeHtml(employee.id)}"`,
        cells: [
          employee.name,
          employee.id,
          (employee.staff_category || 'cornice_hand').replace('_', ' '),
          employee.is_admin ? "Yes" : "No",
          employee.has_password ? "Set" : "No",
          employee.has_fingerprint ? "Enrolled" : "No",
          employee.active ? "Yes" : "No",
        ],
      })),
    ),
  );

  app.querySelector("[data-new-employee]").addEventListener("click", () => {
    state.selectedEmployee = emptyEmployee();
    state.employeeFormMode = "new";
    renderEmployeesPanel();
  });
  app.querySelectorAll("[data-select-employee]").forEach((row) => {
    row.addEventListener("click", () => {
      state.selectedEmployee = employees.find((item) => item.id === row.dataset.selectEmployee);
      state.employeeFormMode = "edit";
      renderEmployeesPanel();
    });
  });
}

function renderEmployeeForm(employees) {
  const mode = state.employeeFormMode === "new" ? "new" : "edit";
  const isNew = mode === "new";
  const selected = state.selectedEmployee || emptyEmployee();
  setPanel(
    mode === "new" ? "Add New Employee" : "Edit Existing Employee",
    `
      <button class="ghost" data-employee-back>Back to Employee List</button>
      <button class="primary" data-save-employee>Save Employee</button>
    `,
    `
      <form class="form-grid" data-employee-form>
        <div class="form-section">
          <h3 class="form-section-title">Details</h3>
          <label><span>Employee ID<span class="req">*</span></span><input name="id" required data-employee-id value="${escapeHtml(selected.id)}" /><span class="field-error" data-id-error></span></label>
          <label><span>Name<span class="req">*</span></span><input name="name" required value="${escapeHtml(selected.name)}" /></label>
          <label><span>Password${isNew ? '<span class="req">*</span>' : ""}</span><input name="password" type="password" ${isNew ? 'required placeholder="Type your password"' : ""} autocomplete="new-password" />${isNew ? "" : '<span class="field-hint">Leave this field empty to keep current password</span>'}</label>
          <label><span>Confirm Password${isNew ? '<span class="req">*</span>' : ""}</span><input name="confirm_password" type="password" ${isNew ? "required" : ""} autocomplete="new-password" /></label>
          <div class="wide password-match" data-password-match></div>
        </div>
        <div class="form-section">
          <h3 class="form-section-title">Account</h3>
          <div class="wide checkbox-row">
            <label class="check"><input type="checkbox" name="active" ${selected.active ? "checked" : ""} /> Active</label>
            <label class="check"><input type="checkbox" name="is_admin" ${selected.is_admin ? "checked" : ""} /> Admin</label>
          </div>
        </div>
        <div class="form-section">
          <h3 class="form-section-title">Role &amp; Permissions</h3>
          <label>Staff Role
            <select name="staff_category">
              ${['cornice_hand','storekeeper','non_cornice','driver','helper'].map(c =>
                `<option value="${c}" ${(selected.staff_category || 'cornice_hand') === c ? 'selected' : ''}>${c.replace('_', ' ')}</option>`
              ).join('')}
            </select>
          </label>
          <div class="wide checkbox-row">
            ${Object.entries(permissionLabels)
              .map(
                ([key, label]) => `
                  <label class="check">
                    <input type="checkbox" name="permission" value="${key}"
                      ${selected.permissions?.includes(key) ? "checked" : ""} />
                    ${escapeHtml(label)}
                  </label>
                `,
              )
              .join("")}
          </div>
        </div>
      </form>
    `,
  );

  app.querySelector("[data-employee-back]").addEventListener("click", () => {
    state.employeeFormMode = null;
    renderEmployeesPanel();
  });

  const employeeForm = app.querySelector("[data-employee-form");
  const idInput = employeeForm.querySelector("[data-employee-id]");
  const idError = employeeForm.querySelector("[data-id-error]");
  const setIdError = (message) => {
    idError.textContent = message;
    idInput.focus();
  };
  idInput.addEventListener("input", () => {
    idError.textContent = "";
  });

  const passwordInput = employeeForm.querySelector('[name="password"]');
  const confirmInput = employeeForm.querySelector('[name="confirm_password"]');
  const matchEl = employeeForm.querySelector("[data-password-match]");
  const updatePasswordMatch = () => {
    const password = passwordInput.value;
    const confirm = confirmInput.value;
    if (!password && !confirm) {
      matchEl.textContent = "";
      matchEl.className = "wide password-match";
      return;
    }
    if (password === confirm) {
      matchEl.textContent = "✓ Password matches";
      matchEl.className = "wide password-match ok";
    } else {
      matchEl.textContent = "✗ Password don't match";
      matchEl.className = "wide password-match err";
    }
  };
  passwordInput.addEventListener("input", updatePasswordMatch);
  confirmInput.addEventListener("input", updatePasswordMatch);

  app.querySelector("[data-save-employee]").addEventListener("click", () => {
    employeeForm.requestSubmit();
  });

  employeeForm.addEventListener("submit", async (event) => {
    event.preventDefault();
    const form = new FormData(event.currentTarget);
    const permissions = form.getAll("permission");
    const newId = String(form.get("id")).trim();
    const originalId = selected.id.trim();
    const password = String(form.get("password") || "");
    const confirmPassword = String(form.get("confirm_password") || "");
    if (
      newId !== originalId &&
      employees.some((item) => item.id.trim().toLowerCase() === newId.toLowerCase())
    ) {
      setIdError("Employee ID already exists.");
      return;
    }
    if (isNew && !password) {
      matchEl.textContent = "✗ Password is required";
      matchEl.className = "wide password-match err";
      passwordInput.focus();
      return;
    }
    if (password && password !== confirmPassword) {
      matchEl.textContent = "✗ Password don't match";
      matchEl.className = "wide password-match err";
      confirmInput.focus();
      return;
    }
    try {
      state.selectedEmployee = await invoke("save_employee", {
        input: {
          id: form.get("id"),
          name: form.get("name"),
          finger: selected.finger || "right-index",
          active: form.get("active") === "on",
          is_admin: form.get("is_admin") === "on",
          password: password || null,
          permissions,
          staff_category: form.get("staff_category") || "cornice_hand",
          expect_new: originalId === "",
        },
      });
    } catch (error) {
      setIdError(String(error.message || error));
      return;
    }
    state.employeeFormMode = null;
    renderEmployeesPanel();
  });
}

export async function renderEnrollPanel() {
  const employees = await invoke("list_staff", { includeInactive: true });
  const selected = state.selectedEmployee || emptyEmployee();
  setPanel(
    "Fingerprint Enrollment",
    `<button class="ghost" data-refresh>Refresh</button>`,
    `
      <form class="form-grid" data-enroll-form>
        <label><span>Employee<span class="req">*</span></span>
          <select name="employee_id">
            <option value="" disabled ${selected.id ? "" : "selected"}>Select an employee</option>
            ${employees
              .map(
                (employee) => `
                  <option value="${escapeHtml(employee.id)}" ${employee.id === selected.id ? "selected" : ""}>
                    ${escapeHtml(employee.name)} (${escapeHtml(employee.id)})
                  </option>
                `,
              )
              .join("")}
          </select>
        </label>
        <label><span>Finger<span class="req">*</span></span>
          <select name="finger">
            <option value="" disabled selected>Select the finger</option>
            ${fingerOptions("")}
          </select>
        </label>
        <div class="wide enroll-actions">
          <button class="warning" type="submit">Enroll / Replace Fingerprint</button>
        </div>
      </form>
      ${table(
        ["Name", "ID", "Admin", "Password", "Fingerprint", "Template Finger", "Last Updated"],
        employees.map((employee) => ({
          review: !employee.has_password || !employee.has_fingerprint,
          cells: [
            employee.name,
            employee.id,
            employee.is_admin ? "Yes" : "No",
            employee.has_password ? "Set" : "No",
            employee.has_fingerprint ? "Enrolled" : "No",
            employee.template_finger || "—",
            formatTimestamp(employee.fingerprint_updated_at),
          ],
        })),
      )}
    `,
  );
  app.querySelector("[data-refresh]").addEventListener("click", renderEnrollPanel);
  app.querySelector("[data-enroll-form]").addEventListener("submit", async (event) => {
    event.preventDefault();
    const button = event.currentTarget.querySelector("button[type='submit']");
    const form = new FormData(event.currentTarget);
    const employeeId = form.get("employee_id");
    const finger = form.get("finger");
    if (!employeeId) {
      await alertModal({ title: "Fingerprint Enrollment", message: "Select an employee to enroll." });
      return;
    }
    if (!finger) {
      await alertModal({ title: "Fingerprint Enrollment", message: "Select the finger to enroll." });
      return;
    }
    const employee = employees.find((entry) => entry.id === employeeId);
    if (!employee) return;
    setBusy(button);
    try {
      await requestEnroll({ employee, finger });
      state.selectedEmployee = employee;
    } catch {
      // Cancelled or failed — the modal already showed the reason.
    } finally {
      setBusy(button, false);
      if (state.adminView === "enroll") renderEnrollPanel();
    }
  });
}
