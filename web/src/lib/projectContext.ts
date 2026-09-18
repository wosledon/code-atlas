import {
  createContext,
  createElement,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from "react";
import { api, type Project, type ProjectsResponse } from "./api";

const STORAGE_KEY = "atlas_active_project";

export function readStoredProjectId(): string | null {
  try {
    const v = localStorage.getItem(STORAGE_KEY);
    return v && v.trim() ? v.trim() : null;
  } catch {
    return null;
  }
}

export function writeStoredProjectId(id: string | null) {
  try {
    if (!id) localStorage.removeItem(STORAGE_KEY);
    else localStorage.setItem(STORAGE_KEY, id);
  } catch {
    /* ignore */
  }
}

type ProjectCtxValue = {
  projectId: string | null;
  setProjectId: (id: string | null) => void;
  projects: Project[];
  loading: boolean;
  refresh: () => Promise<void>;
};

const ProjectCtx = createContext<ProjectCtxValue>({
  projectId: null,
  setProjectId: () => {},
  projects: [],
  loading: false,
  refresh: async () => {},
});

export function useProject() {
  return useContext(ProjectCtx);
}

/** URL `?project=` wins on first load, then localStorage, then first listed project. */
export function ProjectProvider({ children }: { children: ReactNode }) {
  const [projectId, setProjectIdState] = useState<string | null>(() => {
    const fromUrl = new URLSearchParams(window.location.search).get("project");
    return fromUrl && fromUrl.trim() ? fromUrl.trim() : readStoredProjectId();
  });
  const [projects, setProjects] = useState<Project[]>([]);
  const [loading, setLoading] = useState(true);

  const refresh = useCallback(async () => {
    try {
      const r = await api<ProjectsResponse>("/api/projects");
      const list = r.projects || [];
      setProjects(list);
      setProjectIdState((cur) => {
        if (cur && list.some((p) => p.id === cur)) {
          writeStoredProjectId(cur);
          return cur;
        }
        const next = list[0]?.id ?? null;
        writeStoredProjectId(next);
        return next;
      });
    } catch {
      setProjects([]);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const setProjectId = useCallback((id: string | null) => {
    setProjectIdState(id);
    writeStoredProjectId(id);
  }, []);

  const value = useMemo(
    () => ({ projectId, setProjectId, projects, loading, refresh }),
    [projectId, setProjectId, projects, loading, refresh]
  );

  return createElement(ProjectCtx.Provider, { value }, children);
}
