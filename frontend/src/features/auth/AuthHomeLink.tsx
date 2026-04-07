import { House } from "lucide-react";
import { Link } from "react-router-dom";

export function AuthHomeLink() {
  return (
    <Link
      to="/"
      className="platform-page-action-link"
    >
      <House size={16} />
      <span>返回首页</span>
    </Link>
  );
}

export default AuthHomeLink;
